package main

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/rs/zerolog"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/cookies"
	"go.mau.fi/mautrix-meta/pkg/messagix/httpclient"
	"go.mau.fi/mautrix-meta/pkg/messagix/table"
	"go.mau.fi/mautrix-meta/pkg/messagix/types"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/store/sqlstore"
	waTypes "go.mau.fi/whatsmeow/types"
)

const platformMessenger = types.Messenger

// maxUploadBytes matches Capabilities.MaxUploadBytes.
const maxUploadBytes = 26214400

// connectedGrace is how long start waits for the socket's ConnectedEvent before reporting
// online anyway: the HTTP inbox is already loaded at that point, so sends work.
const connectedGrace = 15 * time.Second

// msgMeta indexes one observed message so later table rows (edits, reactions, receipts) and
// RPC calls can be resolved back to their chat and sender without a database.
type msgMeta struct {
	ThreadKey   int64
	ChatID      string
	SenderID    int64
	SenderJID   waTypes.JID
	IsOwn       bool
	TimestampMs int64
	Text        string
	EditCount   int64
}

// threadInfo is the subset of a Messenger thread row the connector needs.
type threadInfo struct {
	Key            int64
	JID            waTypes.JID
	Type           table.ThreadType
	Name           string
	Picture        string
	Folder         string
	LastActivityMs int64
	LastReadMs     int64
	UnreadCount    int64
	MuteExpireMs   int64
}

// contactInfo is the name and profile picture URL of one person.
type contactInfo struct {
	Name    string
	Picture string
}

// Messenger is the connector instance. The host drives it from many goroutines, so every
// mutable field below is guarded by mu.
type Messenger struct {
	log     zerolog.Logger
	out     *writer
	dataDir string
	// settings is the runtime configuration the host passed in the environment; it never changes.
	settings settings

	mu         sync.Mutex
	cookies    *cookies.Cookies
	native     *types.NativeSession
	cli        *messagix.Client
	initial    *table.LSTable
	remoteUser string
	display    string
	ownID      int64
	started    bool
	loggedOut  bool
	online     bool

	threads map[int64]*threadInfo
	// contacts holds what contact rows say about people: one-to-one threads carry no name or
	// picture of their own, so both come from the other person's contact row.
	contacts map[int64]contactInfo
	// members holds who is in each group thread, by thread key then contact id; the value says
	// whether that person is an admin.
	members          map[int64]map[int64]bool
	e2eeThreads      map[string]*threadInfo
	threadsExhausted bool
	messages         map[string]*msgMeta
	txns             map[string]string
	// pinned is the ordered pinned-chat list, top first, by remote chat id. messagix exposes no
	// pin API, so the list lives on this device: see chats.go and pins.json in dataDir.
	pinned []string

	pendingLogin bool
	lite         *liteLogin

	ctx    context.Context
	cancel context.CancelFunc

	e2eeMu      sync.Mutex
	e2ee        *whatsmeow.Client
	deviceStore *sqlstore.Container
	device      *store.Device
	// e2eeRetry is the one pending reconnect, e2eeAttempts counts the failed attempts of the
	// current outage and e2eeBusy says a connect attempt is in flight. All three are guarded by
	// e2eeMu so a flapping socket keeps at most one retry alive.
	e2eeRetry    *time.Timer
	e2eeAttempts int
	e2eeBusy     bool
}

func newMessenger(log zerolog.Logger, out *writer, cfg settings) *Messenger {
	dataDir := os.Getenv("ALLOY_SIDECAR_DATA_DIR")
	if dataDir == "" {
		if configDir, err := os.UserConfigDir(); err == nil {
			dataDir = filepath.Join(configDir, "alloy-meta")
		} else {
			dataDir = filepath.Join(os.TempDir(), "alloy-meta")
		}
	}
	return &Messenger{
		log:         log,
		out:         out,
		dataDir:     dataDir,
		settings:    cfg,
		threads:     make(map[int64]*threadInfo),
		contacts:    make(map[int64]contactInfo),
		members:     make(map[int64]map[int64]bool),
		e2eeThreads: make(map[string]*threadInfo),
		messages:    make(map[string]*msgMeta),
		txns:        make(map[string]string),
		ctx:         context.Background(),
	}
}

func (m *Messenger) emit(event any) {
	if err := m.out.write(EventFrame{Type: "event", Event: event}); err != nil {
		m.log.Err(err).Msg("Failed to write event frame")
	}
}

// handle dispatches one RPC method. A non-nil WireError is reported in the response frame.
func (m *Messenger) handle(method string, params json.RawMessage) (any, *WireError) {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	switch method {
	case "network":
		return networkMessenger, nil
	case "capabilities":
		return MessengerCapabilities(), nil
	case "begin_login":
		return m.beginLogin(), nil
	case "submit_login":
		return m.submitLogin(ctx, params)
	case "restore":
		return m.restore(ctx, params)
	case "start":
		return nil, m.start(ctx)
	case "stop":
		return nil, m.stop()
	case "list_chats":
		return m.listChats(ctx, params)
	case "list_members":
		return m.listMembers(ctx, params)
	case "backfill":
		return m.backfill(ctx, params)
	case "send":
		return m.sendMessage(ctx, params)
	case "edit":
		return m.editMessage(ctx, params)
	case "delete":
		return m.deleteMessage(ctx, params)
	case "react":
		return m.react(ctx, params, true)
	case "unreact":
		return m.react(ctx, params, false)
	case "mark_read":
		return m.markRead(ctx, params)
	case "set_typing":
		return m.setTyping(ctx, params)
	case "set_chat_pinned":
		return m.setChatPinned(params)
	case "reorder_pinned_chats":
		return m.reorderPinnedChats(params)
	case "set_message_pinned":
		// Messenger has no message-pin API the sidecar can reach; the host pins locally.
		return nil, errNotSupported("pin_messages")
	case "fetch_media":
		return m.fetchMedia(ctx, params)
	case "logout":
		m.logout(ctx)
		return nil, nil
	default:
		return nil, errNotSupported(method)
	}
}

// ensureClientLocked builds the messagix client from the stored cookies. mu must be held.
func (m *Messenger) ensureClientLocked() *WireError {
	if m.loggedOut || m.cookies == nil {
		return errUnauthorized()
	}
	if m.cli == nil {
		m.cli = m.settings.newMessagixClient(m.logger(), m.cookies)
		m.cli.MessengerLite.SetNativeSession(m.native)
	}
	return nil
}

func (m *Messenger) logger() zerolog.Logger {
	return m.log.With().Str("component", "messagix").Logger()
}

// loadInboxLocked performs the initial inbox load that authenticates the cookies. mu must be
// held. The returned table is kept for start to consume.
func (m *Messenger) loadInboxLocked(ctx context.Context) *WireError {
	if err := m.ensureClientLocked(); err != nil {
		return err
	}
	user, initial, err := m.cli.LoadMessagesPage(ctx)
	if err != nil {
		return mapMetaError(err)
	}
	m.ownID = user.GetFBID()
	m.remoteUser = strconv.FormatInt(m.ownID, 10)
	m.display = user.GetName()
	m.initial = initial
	m.applyThreadTableLocked(initial)
	m.loadPinsLocked()
	return nil
}

func (m *Messenger) restore(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p RestoreParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("restore params: " + err.Error())
	}
	data, err := decodeSession(p.Session)
	if err != nil {
		return nil, errUnauthorized()
	}
	c, native, err := cookiesFromSession(data)
	if err != nil {
		return nil, errUnauthorized()
	}

	m.mu.Lock()
	m.cookies = c
	m.native = native
	m.cli = m.settings.newMessagixClient(m.logger(), c)
	m.cli.MessengerLite.SetNativeSession(native)
	m.loggedOut = false
	m.mu.Unlock()

	if werr := m.loadInboxLockedGuarded(ctx); werr != nil {
		m.mu.Lock()
		m.cli = nil
		m.cookies = nil
		m.native = nil
		m.mu.Unlock()
		return nil, werr
	}

	m.mu.Lock()
	user := RemoteUser{RemoteUserID: m.remoteUser, DisplayName: m.display}
	m.mu.Unlock()
	return user, nil
}

func (m *Messenger) loadInboxLockedGuarded(ctx context.Context) *WireError {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.loadInboxLocked(ctx)
}

// start connects the socket, streams table updates as events and brings up the E2EE client.
func (m *Messenger) start(ctx context.Context) *WireError {
	m.mu.Lock()
	if m.loggedOut || m.cookies == nil {
		m.mu.Unlock()
		return errUnauthorized()
	}
	if m.started {
		// Replacing the previous sender is allowed; just keep going.
		if m.cancel != nil {
			m.cancel()
		}
	}
	runCtx, cancel := context.WithCancel(context.Background())
	m.ctx = runCtx
	m.cancel = cancel
	m.started = true
	m.online = false
	if err := m.ensureClientLocked(); err != nil {
		m.mu.Unlock()
		return err
	}
	// A fresh login arrives here with the inbox already loaded, so the pinned list is read
	// again: it is this device's own file and re-reading it is harmless after a restore.
	m.loadPinsLocked()
	initial := m.initial
	m.initial = nil
	cli := m.cli
	m.mu.Unlock()

	m.emit(NewConnectionEvent(stateConnecting()))

	if initial == nil {
		if werr := m.loadInboxLockedGuarded(runCtx); werr != nil {
			m.mu.Lock()
			m.started = false
			m.mu.Unlock()
			return werr
		}
		m.mu.Lock()
		initial = m.initial
		m.initial = nil
		m.mu.Unlock()
	}
	if initial != nil {
		m.applyTable(runCtx, initial)
	}

	cli.SetEventHandler(m.handleMessagixEvent)
	if err := cli.Connect(runCtx); err != nil {
		cli.SetEventHandler(nil)
		m.emit(NewConnectionEvent(stateOffline(err.Error(), 1, connectBackoffMillis(1))))
		return mapMetaError(err)
	}

	go m.markOnlineAfter(runCtx, connectedGrace)
	go func() {
		if err := m.startE2EE(runCtx); err != nil {
			m.log.Warn().Err(err).Msg("E2EE client unavailable; encrypted chats will not sync")
		}
	}()
	go m.sessionRotationLoop(runCtx)
	return nil
}

func (m *Messenger) markOnlineAfter(ctx context.Context, d time.Duration) {
	select {
	case <-ctx.Done():
	case <-time.After(d):
		m.mu.Lock()
		already := m.online
		m.mu.Unlock()
		if !already {
			m.markOnline()
		}
	}
}

func (m *Messenger) markOnline() {
	m.mu.Lock()
	if m.online {
		m.mu.Unlock()
		return
	}
	m.online = true
	m.mu.Unlock()
	m.emit(NewConnectionEvent(stateOnline()))
}

// sessionRotationLoop emits SessionUpdated whenever the cookie jar changed (messagix rotates
// the activity cookie on responses). The host persists it so the next launch stays logged in.
func (m *Messenger) sessionRotationLoop(ctx context.Context) {
	ticker := time.NewTicker(2 * time.Minute)
	defer ticker.Stop()
	var last string
	m.mu.Lock()
	if m.cookies != nil {
		last, _ = encodeSession(m.cookies, m.native)
	}
	m.mu.Unlock()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
		m.mu.Lock()
		c := m.cookies
		native := m.native
		m.mu.Unlock()
		if c == nil {
			continue
		}
		current, err := encodeSession(c, native)
		if err != nil || current == last {
			continue
		}
		last = current
		m.emit(NewSessionUpdatedEvent(current))
	}
}

func (m *Messenger) stop() *WireError {
	m.mu.Lock()
	cancel := m.cancel
	m.cancel = nil
	m.started = false
	m.online = false
	cli := m.cli
	m.mu.Unlock()
	if cancel != nil {
		cancel()
	}
	if cli != nil {
		cli.SetEventHandler(nil)
		cli.Disconnect()
	}
	m.e2eeMu.Lock()
	e2ee := m.e2ee
	m.e2ee = nil
	m.cancelE2EERetryLocked()
	m.e2eeMu.Unlock()
	if e2ee != nil {
		e2ee.Disconnect()
	}
	return nil
}

func (m *Messenger) logout(ctx context.Context) {
	m.e2eeMu.Lock()
	e2ee := m.e2ee
	m.e2eeMu.Unlock()
	if e2ee != nil {
		if err := e2ee.Logout(ctx); err != nil {
			m.log.Warn().Err(err).Msg("E2EE remote logout failed; wiping local credentials anyway")
		}
	}
	_ = m.stop()
	m.e2eeMu.Lock()
	device := m.device
	container := m.deviceStore
	m.device = nil
	m.deviceStore = nil
	m.e2eeMu.Unlock()
	if device != nil {
		if err := device.Delete(ctx); err != nil {
			m.log.Warn().Err(err).Msg("Failed to delete WhatsApp device from the store")
		}
	}
	if container != nil {
		_ = container.Close()
	}
	m.mu.Lock()
	user := m.remoteUser
	m.cookies = nil
	m.native = nil
	m.cli = nil
	m.lite = nil
	m.pendingLogin = false
	m.loggedOut = true
	m.messages = make(map[string]*msgMeta)
	m.threads = make(map[int64]*threadInfo)
	m.contacts = make(map[int64]contactInfo)
	m.members = make(map[int64]map[int64]bool)
	m.e2eeThreads = make(map[string]*threadInfo)
	m.pinned = nil
	m.mu.Unlock()
	if user != "" {
		if err := os.RemoveAll(filepath.Join(m.dataDir, user)); err != nil {
			m.log.Warn().Err(err).Msg("Failed to remove the E2EE store")
		}
	}
	if err := os.Remove(m.pinsPath()); err != nil && !errors.Is(err, os.ErrNotExist) {
		m.log.Warn().Err(err).Msg("Failed to remove the pinned-chat list")
	}
}

// handleMessagixEvent receives everything messagix pushes after Connect.
func (m *Messenger) handleMessagixEvent(_ context.Context, raw any) {
	switch evt := raw.(type) {
	case *table.LSTable:
		m.applyTable(m.backgroundCtx(), evt)
	case *messagix.ConnectedEvent:
		m.markOnline()
	case *messagix.ReconnectedEvent:
		m.markOnline()
	case *messagix.TransientDisconnectEvent:
		reason := "connection lost"
		if evt.Err != nil {
			reason = evt.Err.Error()
		}
		m.mu.Lock()
		m.online = false
		m.mu.Unlock()
		// messagix retries on its own and counts the attempt it just failed (1-based since
		// v0.2610.0). Forwarding the count lets the host show real progress instead of "attempt 1"
		// for every report of one outage, and forwarding the wait it will sleep lets the banner
		// show this connector's own schedule instead of a model of it.
		attempt := uint32(0)
		if evt.ConnectionAttempts > 0 {
			attempt = uint32(evt.ConnectionAttempts)
		}
		m.emit(NewConnectionEvent(stateOffline(reason, attempt, connectBackoffMillis(attempt))))
	case *messagix.PermanentErrorEvent:
		m.mu.Lock()
		m.online = false
		m.mu.Unlock()
		m.emit(NewConnectionEvent(stateNeedsRelogin()))
	}
}

// connectBackoffMillis is the delay messagix waits before its next connect attempt after failed
// attempt n (1-based). Its connection loop doubles the wait before sleeping and caps it at
// messagix.MaxConnectBackoff, so the wait is min(2^n s, 5 min): two seconds after the first
// failure. An uncounted attempt (0) is that first failure.
func connectBackoffMillis(attempt uint32) uint64 {
	if attempt == 0 {
		attempt = 1
	}
	// Clamp before the shift so a large count cannot overflow the duration.
	wait := time.Second << min(attempt, 9)
	if wait > messagix.MaxConnectBackoff {
		return uint64(messagix.MaxConnectBackoff / time.Millisecond)
	}
	return uint64(wait / time.Millisecond)
}

func (m *Messenger) backgroundCtx() context.Context {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.ctx
}

// remoteChatID renders a thread key for the host. E2EE threads use the WhatsApp JID in a
// separate namespace so the two id spaces never collide.
func remoteChatID(key int64, threadType table.ThreadType) string {
	if threadType.IsWhatsApp() {
		return "e2ee:" + jidForThread(key).String()
	}
	return strconv.FormatInt(key, 10)
}

func jidForThread(key int64) waTypes.JID {
	return waTypes.JID{User: strconv.FormatInt(key, 10), Server: waTypes.MessengerServer}
}

// parseChatID splits a host-facing chat id into its thread key or WhatsApp JID.
func parseChatID(id string) (key int64, jid waTypes.JID, isE2EE bool, err error) {
	if strings.HasPrefix(id, "e2ee:") {
		jid, err = waTypes.ParseJID(strings.TrimPrefix(id, "e2ee:"))
		if err != nil {
			return 0, waTypes.EmptyJID, true, fmt.Errorf("invalid e2ee chat id %q: %w", id, err)
		}
		return 0, jid, true, nil
	}
	key, err = strconv.ParseInt(id, 10, 64)
	if err != nil {
		return 0, waTypes.EmptyJID, false, fmt.Errorf("invalid chat id %q: %w", id, err)
	}
	return key, waTypes.EmptyJID, false, nil
}

// chatID is the host-facing id of a thread: E2EE chats carry the WhatsApp JID and plain chats
// their numeric thread key.
func (t *threadInfo) chatID() string {
	if t.Type.IsWhatsApp() {
		jid := t.JID
		if jid.IsEmpty() {
			jid = jidForThread(t.Key)
		}
		return "e2ee:" + jid.String()
	}
	return strconv.FormatInt(t.Key, 10)
}

func (m *Messenger) chatForThreadLocked(t *threadInfo) RemoteChat {
	var title = t.Name
	var picture = t.Picture
	key := strconv.FormatInt(t.Key, 10)
	if t.Type.IsOneToOne() {
		// The thread key of a one-to-one chat is the other person's id.
		contact := m.contacts[t.Key]
		if (title == "" || title == key) && contact.Name != "" {
			title = contact.Name
		}
		if picture == "" {
			picture = contact.Picture
		}
	}
	if title == "" {
		title = key
	}
	var avatar *MediaRef
	if picture != "" {
		avatar = &MediaRef{RemoteRef: picture, Mime: "image/jpeg"}
	}
	pinned, pinOrder := m.pinOrderLocked(t.chatID())
	return RemoteChat{
		RemoteChatID:   t.chatID(),
		Kind:           chatKind(t.Type),
		Title:          title,
		Avatar:         avatar,
		LastActivityAt: t.LastActivityMs,
		UnreadCount:    uint32(max(t.UnreadCount, 0)),
		IsPinned:       pinned,
		PinOrder:       pinOrder,
		IsArchived:     strings.Contains(strings.ToLower(t.Folder), "archived"),
		IsMuted:        t.MuteExpireMs > time.Now().UnixMilli(),
	}
}

func chatSnapshot(chat RemoteChat) string {
	raw, err := json.Marshal(chat)
	if err != nil {
		return ""
	}
	return string(raw)
}

func chatKind(t table.ThreadType) string {
	if t.IsOneToOne() {
		return "direct"
	}
	if t == table.FOLDER || t == table.COMMUNITY_FOLDER || t == table.COMMUNITY_CHANNEL_CATEGORY {
		return "channel"
	}
	return "group"
}

// mapMetaError turns messagix/httpclient transport errors into the connector error taxonomy.
func mapMetaError(err error) *WireError {
	if err == nil {
		return nil
	}
	switch {
	case errors.Is(err, httpclient.ErrRateLimited):
		return errRateLimited(30_000)
	case errors.Is(err, httpclient.ErrTokenInvalidated),
		errors.Is(err, httpclient.ErrUserIDIsZero),
		errors.Is(err, httpclient.ErrCheckpointRequired),
		errors.Is(err, httpclient.ErrChallengeRequired),
		errors.Is(err, httpclient.ErrConsentRequired),
		errors.Is(err, httpclient.ErrAccountSuspended):
		return errUnauthorized()
	case errors.Is(err, context.Canceled), errors.Is(err, context.DeadlineExceeded),
		errors.Is(err, httpclient.ErrUnexpectedError):
		return errNetwork(err.Error())
	}
	var netErr interface{ Timeout() bool }
	if errors.As(err, &netErr) {
		return errNetwork(err.Error())
	}
	msg := err.Error()
	if strings.Contains(msg, "connection") || strings.Contains(msg, "network") ||
		strings.Contains(msg, "tls:") || strings.Contains(msg, "dial ") ||
		strings.Contains(msg, "no such host") {
		return errNetwork(msg)
	}
	return errProtocol(msg)
}

func pageCursor(ts, key int64) string {
	raw, _ := json.Marshal(struct {
		TS  int64 `json:"ts"`
		Key int64 `json:"key"`
	}{TS: ts, Key: key})
	return base64.RawURLEncoding.EncodeToString(raw)
}

func decodeCursor(cursor string) (ts, key int64, err error) {
	raw, err := base64.RawURLEncoding.DecodeString(cursor)
	if err != nil {
		return 0, 0, fmt.Errorf("malformed cursor")
	}
	var decoded struct {
		TS  int64 `json:"ts"`
		Key int64 `json:"key"`
	}
	if err := json.Unmarshal(raw, &decoded); err != nil {
		return 0, 0, fmt.Errorf("malformed cursor")
	}
	return decoded.TS, decoded.Key, nil
}

// applyThreadTableLocked folds thread rows from a table into the local chat index. mu must be
// held.
func (m *Messenger) applyThreadTableLocked(tbl *table.LSTable) {
	if tbl == nil {
		return
	}
	m.applyContactsLocked(tbl)
	// A resync clears a thread's participants and adds them again in the same table.
	for _, row := range tbl.LSRemoveAllParticipantsForThread {
		delete(m.members, row.ThreadKey)
	}
	for _, row := range tbl.LSAddParticipantIdToGroupThread {
		if m.members[row.ThreadKey] == nil {
			m.members[row.ThreadKey] = make(map[int64]bool)
		}
		m.members[row.ThreadKey][row.ContactId] = row.IsAdmin || row.IsSuperAdmin
	}
	for _, row := range tbl.LSRemoveParticipantFromThread {
		delete(m.members[row.ThreadKey], row.ParticipantId)
	}
	for _, row := range tbl.LSUpdateThreadParticipantAdminStatus {
		if _, ok := m.members[row.ThreadKey][row.ContactId]; ok {
			m.members[row.ThreadKey][row.ContactId] = row.IsAdmin
		}
	}
	for _, row := range tbl.LSOverwriteAllThreadParticipantsAdminStatus {
		for id := range m.members[row.ThreadKey] {
			m.members[row.ThreadKey][id] = row.IsAdmin
		}
	}
	for _, row := range tbl.LSDeleteThenInsertThread {
		m.upsertThreadLocked(row.ThreadKey, row.ThreadType, row.ThreadName, row.ThreadPictureUrl,
			row.FolderName, row.LastActivityTimestampMs, row.LastReadWatermarkTimestampMs,
			row.UnreadMessageCount, row.MuteExpireTimeMs)
	}
	for _, row := range tbl.LSUpdateOrInsertThread {
		m.upsertThreadLocked(row.ThreadKey, row.ThreadType, row.ThreadName, row.ThreadPictureUrl,
			row.FolderName, row.LastActivityTimestampMs, row.LastReadWatermarkTimestampMs,
			0, row.MuteExpireTimeMs)
	}
	for _, row := range tbl.LSVerifyThreadExists {
		if m.threads[row.ThreadKey] != nil {
			continue
		}
		m.upsertThreadLocked(row.ThreadKey, row.ThreadType, "", "", row.FolderName, 0, 0, 0, 0)
	}
}

// applyContactsLocked folds the contact rows of a table into the contact index. mu must be held.
func (m *Messenger) applyContactsLocked(tbl *table.LSTable) {
	for _, row := range tbl.LSVerifyContactRowExists {
		m.upsertContactLocked(row.ContactId, row.Name, row.ProfilePictureUrl)
	}
	for _, row := range tbl.LSDeleteThenInsertContact {
		m.upsertContactLocked(row.Id, row.Name, row.ProfilePictureUrl)
	}
}

// profileLocked is what the contact rows say about one person, or nil while their name is
// unknown. mu must be held.
func (m *Messenger) profileLocked(id int64) *RemoteUser {
	contact := m.contacts[id]
	name := contact.Name
	if name == "" && id == m.ownID {
		name = m.display
	}
	if name == "" {
		return nil
	}
	user := &RemoteUser{RemoteUserID: strconv.FormatInt(id, 10), DisplayName: name}
	if contact.Picture != "" {
		user.Avatar = &MediaRef{RemoteRef: contact.Picture, Mime: "image/jpeg"}
	}
	return user
}

// upsertContactLocked remembers a person's name and picture; empty fields keep what is known.
func (m *Messenger) upsertContactLocked(id int64, name, picture string) {
	contact := m.contacts[id]
	if name != "" {
		contact.Name = name
	}
	if picture != "" {
		contact.Picture = picture
	}
	m.contacts[id] = contact
}

func (m *Messenger) upsertThreadLocked(key int64, threadType table.ThreadType, name, picture, folder string, activity, read, unread, mute int64) {
	if key == 0 {
		return
	}
	t := m.threads[key]
	if t == nil {
		t = &threadInfo{Key: key}
		m.threads[key] = t
	}
	if threadType != table.UNKNOWN_THREAD_TYPE {
		t.Type = threadType
	}
	if name != "" {
		t.Name = name
	}
	if picture != "" {
		t.Picture = picture
	}
	if folder != "" {
		t.Folder = folder
	}
	if activity != 0 {
		t.LastActivityMs = activity
	}
	t.LastReadMs = read
	t.UnreadCount = unread
	t.MuteExpireMs = mute
}

// applyTable translates one messagix table into connector events. The caller owns no locks.
func (m *Messenger) applyTable(ctx context.Context, tbl *table.LSTable) {
	if tbl == nil {
		return
	}
	var events []any
	m.mu.Lock()
	emitting := m.started
	touched := make(map[int64]bool, len(tbl.LSDeleteThenInsertThread)+len(tbl.LSUpdateOrInsertThread))
	for _, row := range tbl.LSDeleteThenInsertThread {
		touched[row.ThreadKey] = true
	}
	for _, row := range tbl.LSUpdateOrInsertThread {
		touched[row.ThreadKey] = true
	}
	for _, row := range tbl.LSVerifyThreadExists {
		touched[row.ThreadKey] = true
	}
	// A contact row changes the title and picture of that person's one-to-one chat.
	for _, row := range tbl.LSVerifyContactRowExists {
		touched[row.ContactId] = true
	}
	for _, row := range tbl.LSDeleteThenInsertContact {
		touched[row.Id] = true
	}
	before := make(map[int64]string, len(touched))
	for key := range touched {
		if t := m.threads[key]; t != nil {
			before[key] = chatSnapshot(m.chatForThreadLocked(t))
		}
	}
	m.applyThreadTableLocked(tbl)
	for key := range touched {
		t := m.threads[key]
		if t == nil {
			continue
		}
		chat := m.chatForThreadLocked(t)
		if before[key] != chatSnapshot(chat) {
			events = append(events, NewChatUpsertedEvent(chat))
		}
	}
	for _, row := range tbl.LSDeleteThread {
		events = append(events, m.removeThreadLocked(row.ThreadKey))
	}
	for _, row := range tbl.LSHybridThreadDelete {
		events = append(events, m.removeThreadLocked(row.ThreadKey))
	}
	for _, row := range tbl.LSDeletePartialThread {
		events = append(events, m.removeThreadLocked(row.ThreadKey))
	}
	events = append(events, m.messageEventsLocked(tbl)...)
	m.mu.Unlock()

	if !emitting {
		return
	}
	for _, evt := range events {
		if evt != nil {
			m.emit(evt)
		}
	}
	_ = ctx
}

func (m *Messenger) removeThreadLocked(key int64) any {
	t, ok := m.threads[key]
	if !ok {
		return nil
	}
	delete(m.threads, key)
	delete(m.members, key)
	if index := slices.Index(m.pinned, t.chatID()); index >= 0 {
		// A chat that is gone leaves the pinned list so the others keep contiguous positions.
		m.pinned = slices.Delete(slices.Clone(m.pinned), index, index+1)
		if werr := m.persistPinsLocked(); werr != nil {
			m.log.Warn().Str("error", werr.Error()).Msg("Failed to persist the pinned-chat list")
		}
	}
	return NewChatRemovedEvent(t.chatID())
}

// allThreadsLocked returns every chat, plain and E2EE. mu must be held.
func (m *Messenger) allThreadsLocked() []*threadInfo {
	out := make([]*threadInfo, 0, len(m.threads)+len(m.e2eeThreads))
	for _, t := range m.threads {
		out = append(out, t)
	}
	for _, t := range m.e2eeThreads {
		out = append(out, t)
	}
	return out
}

// threadTypeKnown reports whether the thread exists with a usable type.
func (m *Messenger) threadTypeKnown(key int64) (table.ThreadType, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	t := m.threads[key]
	if t == nil {
		return table.UNKNOWN_THREAD_TYPE, false
	}
	return t.Type, true
}
