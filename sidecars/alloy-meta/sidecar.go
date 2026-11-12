package main

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
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

// Messenger is the connector instance. The host drives it from many goroutines, so every
// mutable field below is guarded by mu.
type Messenger struct {
	log     zerolog.Logger
	out     *writer
	dataDir string

	mu         sync.Mutex
	cookies    *cookies.Cookies
	cli        *messagix.Client
	initial    *table.LSTable
	remoteUser string
	display    string
	ownID      int64
	started    bool
	loggedOut  bool
	online     bool

	threads          map[int64]*threadInfo
	e2eeThreads      map[string]*threadInfo
	threadsExhausted bool
	messages         map[string]*msgMeta
	txns             map[string]string

	pendingLogin bool

	ctx    context.Context
	cancel context.CancelFunc

	e2eeMu      sync.Mutex
	e2ee        *whatsmeow.Client
	deviceStore *sqlstore.Container
	device      *store.Device
}

func newMessenger(log zerolog.Logger, out *writer) *Messenger {
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
		threads:     make(map[int64]*threadInfo),
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
		m.cli = messagix.NewClient(m.cookies, m.logger(), &messagix.Config{})
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
	c, err := cookiesFromSession(data)
	if err != nil {
		return nil, errUnauthorized()
	}

	m.mu.Lock()
	m.cookies = c
	m.cli = messagix.NewClient(c, m.logger(), &messagix.Config{})
	m.loggedOut = false
	m.mu.Unlock()

	if werr := m.loadInboxLockedGuarded(ctx); werr != nil {
		m.mu.Lock()
		m.cli = nil
		m.cookies = nil
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
		m.emit(NewConnectionEvent(stateOffline(err.Error())))
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
		last, _ = encodeSession("messenger", m.cookies)
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
		m.mu.Unlock()
		if c == nil {
			continue
		}
		current, err := encodeSession("messenger", c)
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
	m.cli = nil
	m.loggedOut = true
	m.messages = make(map[string]*msgMeta)
	m.threads = make(map[int64]*threadInfo)
	m.e2eeThreads = make(map[string]*threadInfo)
	m.mu.Unlock()
	if user != "" {
		if err := os.RemoveAll(filepath.Join(m.dataDir, user)); err != nil {
			m.log.Warn().Err(err).Msg("Failed to remove the E2EE store")
		}
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
		m.emit(NewConnectionEvent(stateOffline(reason)))
	case *messagix.PermanentErrorEvent:
		m.mu.Lock()
		m.online = false
		m.mu.Unlock()
		m.emit(NewConnectionEvent(stateNeedsRelogin()))
	}
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
	if title == "" {
		title = strconv.FormatInt(t.Key, 10)
	}
	var avatar *MediaRef
	if t.Picture != "" {
		avatar = &MediaRef{RemoteRef: t.Picture, Mime: "image/jpeg"}
	}
	return RemoteChat{
		RemoteChatID:   t.chatID(),
		Kind:           chatKind(t.Type),
		Title:          title,
		Avatar:         avatar,
		LastActivityAt: t.LastActivityMs,
		UnreadCount:    uint32(max(t.UnreadCount, 0)),
		IsPinned:       false,
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
