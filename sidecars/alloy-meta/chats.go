package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strconv"
	"strings"
	"time"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/dgw"
	"go.mau.fi/mautrix-meta/pkg/messagix/methods"
	"go.mau.fi/mautrix-meta/pkg/messagix/socket"
	"go.mau.fi/mautrix-meta/pkg/messagix/table"
	waTypes "go.mau.fi/whatsmeow/types"
)

// listChats returns chats newest-activity-first. The cursor is an opaque base64 value holding
// the last returned (activity, thread key) pair, so paging is stable while the set changes.
func (m *Messenger) listChats(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p ListChatsParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("list_chats params: " + err.Error())
	}
	if p.Limit == 0 {
		return nil, errInvalidInput("limit must be greater than zero")
	}
	limit := int(p.Limit)
	if limit < 0 || limit > 1000 {
		return nil, errInvalidInput("limit must be between 1 and 1000")
	}

	var cursorTS, cursorKey int64
	hasCursor := false
	if p.Cursor != nil {
		var err error
		cursorTS, cursorKey, err = decodeCursor(*p.Cursor)
		if err != nil {
			return nil, errInvalidInput(err.Error())
		}
		hasCursor = true
	}

	if werr := m.ensureThreads(ctx, limit); werr != nil {
		return nil, werr
	}

	m.mu.Lock()
	if m.cli == nil || m.loggedOut {
		m.mu.Unlock()
		return nil, errUnauthorized()
	}
	all := make([]RemoteChat, 0, len(m.threads)+len(m.e2eeThreads))
	for _, t := range m.allThreadsLocked() {
		all = append(all, m.chatForThreadLocked(t))
	}
	m.mu.Unlock()

	sort.SliceStable(all, func(i, j int) bool {
		leftTS, leftKey := chatSortKey(all[i])
		rightTS, rightKey := chatSortKey(all[j])
		if leftTS != rightTS {
			return leftTS > rightTS
		}
		return leftKey > rightKey
	})

	eligible := make([]RemoteChat, 0, len(all))
	for _, chat := range all {
		if hasCursor {
			ts, key := chatSortKey(chat)
			if ts > cursorTS || (ts == cursorTS && key >= cursorKey) {
				continue
			}
		}
		eligible = append(eligible, chat)
	}
	if len(eligible) > limit {
		eligible = eligible[:limit]
	}
	var next *string
	if len(eligible) == limit {
		// Only promise another page when the full local index suggests older chats exist.
		lastTS, lastKey := chatSortKey(eligible[len(eligible)-1])
		for _, chat := range all {
			ts, key := chatSortKey(chat)
			if ts < lastTS || (ts == lastTS && key < lastKey) {
				cursor := pageCursor(lastTS, lastKey)
				next = &cursor
				break
			}
		}
	}
	return ChatPage{Items: eligible, NextCursor: next}, nil
}

// chatSortKey extracts the comparable portion of a chat id: activity plus a numeric key for
// thread chats; E2EE chat ids order after numeric ones at the same timestamp.
func chatSortKey(chat RemoteChat) (int64, int64) {
	id := strings.TrimPrefix(chat.RemoteChatID, "e2ee:")
	if at := strings.IndexByte(id, '@'); at >= 0 {
		id = id[:at]
	}
	if key, err := strconv.ParseInt(id, 10, 64); err == nil {
		return chat.LastActivityAt, key
	}
	// Non-numeric ids still need a stable position; fold the bytes.
	var folded int64
	for _, b := range []byte(chat.RemoteChatID) {
		folded = folded*31 + int64(b)
	}
	return chat.LastActivityAt, folded
}

// threadFetchWait bounds how long paging waits for the DGW socket to come up before the chat list
// is reported incomplete. start returns as soon as Connect accepts the attempt, so the first
// list_chats after launch arrives while the socket is still dialing.
const (
	threadFetchWait = 10 * time.Second
	threadFetchPoll = 250 * time.Millisecond
)

// socketNotReady reports whether err says the DGW socket has not come up yet rather than that the
// page request failed. messagix returns dgw.ErrSocketNotOpen from every stream attempt while the
// socket is down; wrapped forms spell the same thing out instead of wrapping the sentinel.
func socketNotReady(err error) bool {
	if err == nil {
		return false
	}
	if errors.Is(err, dgw.ErrSocketNotOpen) {
		return true
	}
	msg := strings.ToLower(err.Error())
	return strings.Contains(msg, "socket is not open") || strings.Contains(msg, "not connected")
}

// waitThreadSocket polls until the client's DGW socket is open, the bound runs out, or ctx ends,
// and reports whether the socket came up. A cancelled ctx returns false at once, with no wait.
func waitThreadSocket(ctx context.Context, cli *messagix.Client) bool {
	if cli.IsConnected() {
		return true
	}
	deadline := time.NewTimer(threadFetchWait)
	defer deadline.Stop()
	poll := time.NewTicker(threadFetchPoll)
	defer poll.Stop()
	for {
		select {
		case <-ctx.Done():
			return false
		case <-deadline.C:
			return cli.IsConnected()
		case <-poll.C:
			if cli.IsConnected() {
				return true
			}
		}
	}
}

// threadPagingError renders a paging failure for the host. The list is short, so list_chats fails
// instead of answering with what it has; the cause's type survives so rate limits and logins still
// reach the host as themselves. A socket that never came up is transport, not a protocol fault.
func threadPagingError(err error) *WireError {
	if socketNotReady(err) {
		return errNetwork("chat list incomplete: fetch more threads: " + err.Error())
	}
	werr := mapMetaError(err)
	if werr.Type == "network" || werr.Type == "protocol" {
		werr.Message = "chat list incomplete: fetch more threads: " + werr.Message
	}
	return werr
}

// ensureThreads asks the network for one more thread page when the local index is short and
// returns an error when the list cannot be completed. mu is not held across the call.
func (m *Messenger) ensureThreads(ctx context.Context, want int) *WireError {
	m.mu.Lock()
	if m.cli == nil || m.loggedOut {
		m.mu.Unlock()
		return errUnauthorized()
	}
	count := len(m.threads)
	exhausted := m.threadsExhausted
	cli := m.cli
	m.mu.Unlock()
	if count >= want || exhausted {
		return nil
	}

	waited := false
	for count < want {
		if err := ctx.Err(); err != nil {
			return mapMetaError(err)
		}
		keyStore, tbl, err := cli.FetchMoreThreads(ctx, 1)
		if err != nil {
			if ctxErr := ctx.Err(); ctxErr != nil {
				return mapMetaError(ctxErr)
			}
			if socketNotReady(err) && !waited {
				// The socket was still dialing, not the page failing: one bounded wait per call.
				waited = true
				if waitThreadSocket(ctx, cli) {
					continue
				}
				if ctxErr := ctx.Err(); ctxErr != nil {
					return mapMetaError(ctxErr)
				}
			}
			m.log.Warn().Err(err).Bool("socket_open", cli.IsConnected()).
				Msg("Failed to fetch more threads; the chat list is incomplete")
			return threadPagingError(err)
		}
		if tbl == nil || keyStore == nil {
			m.mu.Lock()
			m.threadsExhausted = true
			m.mu.Unlock()
			return nil
		}
		m.applyTable(ctx, tbl)
		m.mu.Lock()
		next := len(m.threads)
		m.mu.Unlock()
		if next <= count {
			m.mu.Lock()
			m.threadsExhausted = true
			m.mu.Unlock()
			return nil
		}
		count = next
	}
	return nil
}

// pinsFile holds the ordered pinned-chat list in dataDir. messagix exposes no pin API, so
// Messenger pins live on this device only and never reach the network or other devices.
const pinsFile = "pins.json"

func (m *Messenger) pinsPath() string {
	return filepath.Join(m.dataDir, pinsFile)
}

// loadPinsLocked reads the pinned list persisted by an earlier run. A missing or unreadable
// file leaves the list empty; the host then sees no pinned chats. mu must be held.
func (m *Messenger) loadPinsLocked() {
	m.pinned = nil
	raw, err := os.ReadFile(m.pinsPath())
	if err != nil {
		if !errors.Is(err, os.ErrNotExist) {
			m.log.Warn().Err(err).Msg("Failed to read the pinned-chat list")
		}
		return
	}
	var ids []string
	if err := json.Unmarshal(raw, &ids); err != nil {
		m.log.Warn().Err(err).Msg("Ignoring a malformed pinned-chat list")
		return
	}
	for _, id := range ids {
		if _, _, _, err := parseChatID(id); err == nil && !slices.Contains(m.pinned, id) {
			m.pinned = append(m.pinned, id)
		}
	}
}

// persistPinsLocked writes the pinned list so it survives a restart. mu must be held.
func (m *Messenger) persistPinsLocked() *WireError {
	ids := m.pinned
	if ids == nil {
		ids = []string{}
	}
	raw, err := json.Marshal(ids)
	if err != nil {
		return errProtocol("encode pinned-chat list: " + err.Error())
	}
	if err := os.MkdirAll(m.dataDir, 0o700); err != nil {
		return errProtocol("create data directory: " + err.Error())
	}
	if err := os.WriteFile(m.pinsPath(), raw, 0o600); err != nil {
		return errProtocol("write pinned-chat list: " + err.Error())
	}
	return nil
}

// pinOrderLocked reports whether a chat is pinned and its position, 0 at the top. mu must be held.
func (m *Messenger) pinOrderLocked(chatID string) (bool, *uint32) {
	index := slices.Index(m.pinned, chatID)
	if index < 0 {
		return false, nil
	}
	return true, new(uint32(index))
}

// threadForChatIDLocked finds the thread behind a host-facing chat id, plain or E2EE. mu must
// be held.
func (m *Messenger) threadForChatIDLocked(chatID string) *threadInfo {
	threadKey, jid, isE2EE, err := parseChatID(chatID)
	if err != nil {
		return nil
	}
	if !isE2EE {
		return m.threads[threadKey]
	}
	if t := m.e2eeThreads[chatID]; t != nil {
		return t
	}
	if t := m.threads[int64(jid.UserInt())]; t != nil && t.chatID() == chatID {
		return t
	}
	return nil
}

// setChatPinned pins a chat to the top of the local pinned list or removes it. Idempotent.
func (m *Messenger) setChatPinned(params json.RawMessage) (any, *WireError) {
	var p SetChatPinnedParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("set_chat_pinned params: " + err.Error())
	}
	if _, _, _, err := parseChatID(p.RemoteChatID); err != nil {
		return nil, errInvalidInput(err.Error())
	}
	m.mu.Lock()
	if m.cli == nil || m.loggedOut {
		m.mu.Unlock()
		return nil, errUnauthorized()
	}
	if m.threadForChatIDLocked(p.RemoteChatID) == nil {
		m.mu.Unlock()
		return nil, errInvalidInput(fmt.Sprintf("unknown chat %q", p.RemoteChatID))
	}
	index := slices.Index(m.pinned, p.RemoteChatID)
	if p.Pinned == (index >= 0) {
		m.mu.Unlock()
		return nil, nil
	}
	if p.Pinned {
		m.pinned = append([]string{p.RemoteChatID}, m.pinned...)
	} else {
		m.pinned = slices.Delete(slices.Clone(m.pinned), index, index+1)
	}
	werr := m.persistPinsLocked()
	m.mu.Unlock()
	if werr != nil {
		return nil, werr
	}
	m.emit(NewChatPinnedEvent(p.RemoteChatID, p.Pinned))
	return nil, nil
}

// reorderPinnedChats replaces the local pinned list, top first. Every id must name a known
// chat; a chat left out is unpinned.
func (m *Messenger) reorderPinnedChats(params json.RawMessage) (any, *WireError) {
	var p ReorderPinnedChatsParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("reorder_pinned_chats params: " + err.Error())
	}
	ids := make([]string, 0, len(p.RemoteChatIDs))
	for _, id := range p.RemoteChatIDs {
		if _, _, _, err := parseChatID(id); err != nil {
			return nil, errInvalidInput(err.Error())
		}
		if slices.Contains(ids, id) {
			return nil, errInvalidInput(fmt.Sprintf("chat %q listed twice", id))
		}
		ids = append(ids, id)
	}
	m.mu.Lock()
	if m.cli == nil || m.loggedOut {
		m.mu.Unlock()
		return nil, errUnauthorized()
	}
	for _, id := range ids {
		if m.threadForChatIDLocked(id) == nil {
			m.mu.Unlock()
			return nil, errInvalidInput(fmt.Sprintf("unknown chat %q", id))
		}
	}
	m.pinned = ids
	werr := m.persistPinsLocked()
	m.mu.Unlock()
	if werr != nil {
		return nil, werr
	}
	m.emit(NewPinnedChatsReorderedEvent(slices.Clone(ids)))
	return nil, nil
}

// listMembers answers from what the network already sent: the participant rows of a thread, or
// the group info of an encrypted group. A one-to-one chat has no members to list.
func (m *Messenger) listMembers(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p ListMembersParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("list_members params: " + err.Error())
	}
	threadKey, jid, isE2EE, err := parseChatID(p.RemoteChatID)
	if err != nil {
		return nil, errInvalidInput(err.Error())
	}
	m.mu.Lock()
	if m.cli == nil || m.loggedOut {
		m.mu.Unlock()
		return nil, errUnauthorized()
	}
	thread := m.threads[threadKey]
	if isE2EE {
		threadKey = int64(jid.UserInt())
		if thread = m.e2eeThreads[p.RemoteChatID]; thread == nil {
			thread = m.threads[threadKey]
		}
	}
	m.mu.Unlock()
	if thread == nil {
		return nil, errInvalidInput(fmt.Sprintf("unknown chat %q", p.RemoteChatID))
	}
	if thread.Type.IsOneToOne() {
		return []RemoteMember{}, nil
	}

	admins := make(map[int64]bool)
	if jid.Server == waTypes.GroupServer {
		m.e2eeMu.Lock()
		e2ee := m.e2ee
		m.e2eeMu.Unlock()
		if e2ee != nil {
			info, err := e2ee.GetGroupInfo(ctx, jid)
			if err != nil {
				m.log.Warn().Err(err).Msg("Failed to fetch the encrypted group's members")
			} else {
				for _, participant := range info.Participants {
					admins[int64(participant.JID.UserInt())] = participant.IsAdmin || participant.IsSuperAdmin
				}
			}
		}
	}

	m.mu.Lock()
	defer m.mu.Unlock()
	if len(admins) == 0 {
		admins = m.members[threadKey]
	}
	if len(admins) == 0 {
		// Nothing the network sent lists this group; the host keeps what it has.
		return nil, errNotSupported("members")
	}
	return m.membersLocked(admins), nil
}

// membersLocked renders a member set, ordered by id so the answer is stable. mu must be held.
func (m *Messenger) membersLocked(admins map[int64]bool) []RemoteMember {
	ids := make([]int64, 0, len(admins))
	for id := range admins {
		ids = append(ids, id)
	}
	slices.Sort(ids)
	members := make([]RemoteMember, 0, len(ids))
	for _, id := range ids {
		member := RemoteMember{User: RemoteUser{RemoteUserID: strconv.FormatInt(id, 10)}}
		if profile := m.profileLocked(id); profile != nil {
			member.User = *profile
		}
		if admins[id] {
			member.Role = new("admin")
		}
		members = append(members, member)
	}
	return members
}

// backfill pages history backwards through the FetchMessagesTask. Messenger's history request
// is asynchronous on the socket, so the response table is the only page this call can see.
func (m *Messenger) backfill(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p BackfillParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("backfill params: " + err.Error())
	}
	if p.Limit == 0 {
		return nil, errInvalidInput("limit must be greater than zero")
	}
	limit := int(p.Limit)
	if limit < 0 || limit > 1000 {
		return nil, errInvalidInput("limit must be between 1 and 1000")
	}
	threadKey, jid, isE2EE, err := parseChatID(p.RemoteChatID)
	if err != nil {
		return nil, errInvalidInput(err.Error())
	}
	if isE2EE {
		// WhatsApp history for Messenger E2EE chats only exists in Meta's encrypted archive.
		return m.backfillArchive(ctx, p, jid, limit)
	}

	m.mu.Lock()
	cli := m.cli
	if cli == nil || m.loggedOut {
		m.mu.Unlock()
		return nil, errUnauthorized()
	}
	thread := m.threads[threadKey]
	chatID := p.RemoteChatID
	if thread != nil {
		chatID = thread.chatID()
	}
	m.mu.Unlock()
	if thread == nil {
		return nil, errInvalidInput(fmt.Sprintf("unknown chat %q", p.RemoteChatID))
	}

	var refTS int64
	var refID string
	if p.Before != nil {
		refID = *p.Before
		parsed, err := methods.ParseMessageID(refID)
		if err != nil {
			if err := m.fillReferenceFromIndex(refID, &refTS); err != nil {
				return nil, errInvalidInput(fmt.Sprintf("unknown message id %q", refID))
			}
		} else {
			refTS = parsed
		}
	}

	resp, err := cli.ExecuteTasks(ctx, &socket.FetchMessagesTask{
		ThreadKey:            threadKey,
		Direction:            0,
		ReferenceTimestampMs: refTS,
		ReferenceMessageId:   refID,
		SyncGroup:            1,
		Cursor:               cli.GetCursor(1),
	})
	if err != nil {
		return nil, mapMetaError(err)
	}

	refs := attachmentsByMessage(resp)
	seen := make(map[string]bool)
	items := make([]RemoteMessage, 0, len(resp.LSInsertMessage)+len(resp.LSUpsertMessage))
	appendRow := func(row *table.LSInsertMessage) {
		if row == nil || row.ThreadKey != threadKey || row.IsUnsent || seen[row.MessageId] {
			return
		}
		seen[row.MessageId] = true
		msg, _ := m.messageFromRowLocked(row, refs)
		msg.RemoteChatID = chatID
		if p.Before != nil && !olderThan(msg, refTS, refID) {
			return
		}
		items = append(items, msg)
	}
	m.mu.Lock()
	// The page names its senders in its own contact rows.
	m.applyContactsLocked(resp)
	for _, row := range resp.LSInsertMessage {
		appendRow(row)
	}
	for _, row := range resp.LSUpsertMessage {
		appendRow(row.ToInsert())
	}
	for _, row := range resp.LSDeleteThenInsertMessage {
		appendRow(insertFromDeleteThenInsert(row))
	}
	m.mu.Unlock()

	sort.SliceStable(items, func(i, j int) bool {
		if items[i].SentAt != items[j].SentAt {
			return items[i].SentAt > items[j].SentAt
		}
		return items[i].RemoteMsgID > items[j].RemoteMsgID
	})
	if len(items) > limit {
		items = items[:limit]
	}
	var next *string
	if len(items) == limit {
		next = &items[len(items)-1].RemoteMsgID
	}
	return MessagePage{Items: items, NextCursor: next}, nil
}

func (m *Messenger) fillReferenceFromIndex(msgID string, ts *int64) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if meta := m.messages[msgID]; meta != nil {
		*ts = meta.TimestampMs
		return nil
	}
	return fmt.Errorf("unknown message")
}

// olderThan reports whether msg sorts strictly before the (ts, id) anchor.
func olderThan(msg RemoteMessage, ts int64, id string) bool {
	if msg.SentAt != ts {
		return msg.SentAt < ts
	}
	return msg.RemoteMsgID < id
}
