package main

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"strconv"
	"strings"

	"go.mau.fi/mautrix-meta/pkg/messagix/methods"
	"go.mau.fi/mautrix-meta/pkg/messagix/socket"
	"go.mau.fi/mautrix-meta/pkg/messagix/table"
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

// ensureThreads asks the network for one more thread page when the local index is short.
// mu is not held across the call.
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

	for count < want {
		keyStore, tbl, err := cli.FetchMoreThreads(ctx, 1)
		if err != nil {
			m.log.Warn().Err(err).Msg("Failed to fetch more threads")
			return nil
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
	threadKey, _, isE2EE, err := parseChatID(p.RemoteChatID)
	if err != nil {
		return nil, errInvalidInput(err.Error())
	}
	if isE2EE {
		// WhatsApp history for Messenger E2EE chats is not exposed by the socket API.
		return nil, errNotSupported("backfill")
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
