package main

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"image"
	_ "image/gif"
	_ "image/jpeg"
	_ "image/png"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"time"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/methods"
	"go.mau.fi/mautrix-meta/pkg/messagix/table"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waConsumerApplication"
	"go.mau.fi/whatsmeow/proto/waMediaTransport"
	"go.mau.fi/whatsmeow/proto/waMsgApplication"
	"go.mau.fi/whatsmeow/store/sqlstore"
	waTypes "go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	waLog "go.mau.fi/whatsmeow/util/log"
)

// e2eeMediaRef is the opaque MediaRef.remote_ref used for encrypted attachments. It carries
// exactly what fetch_media needs to decrypt and download, and nothing the host should read.
type e2eeMediaRef struct {
	E2EE          bool   `json:"e2ee"`
	MediaType     string `json:"media_type"`
	DirectPath    string `json:"direct_path"`
	MediaKey      []byte `json:"media_key"`
	FileSHA256    []byte `json:"file_sha256"`
	FileEncSHA256 []byte `json:"file_enc_sha256"`
}

const e2eeRefPrefix = "e2ee:"

func encodeE2EEMediaRef(mediaType whatsmeow.MediaType, path string, key, fileHash, encHash []byte) string {
	raw, _ := json.Marshal(e2eeMediaRef{
		E2EE:          true,
		MediaType:     string(mediaType),
		DirectPath:    path,
		MediaKey:      key,
		FileSHA256:    fileHash,
		FileEncSHA256: encHash,
	})
	return e2eeRefPrefix + base64.RawURLEncoding.EncodeToString(raw)
}

func decodeE2EEMediaRef(ref string) (*e2eeMediaRef, error) {
	if len(ref) <= len(e2eeRefPrefix) || ref[:len(e2eeRefPrefix)] != e2eeRefPrefix {
		return nil, fmt.Errorf("not an e2ee media reference")
	}
	raw, err := base64.RawURLEncoding.DecodeString(ref[len(e2eeRefPrefix):])
	if err != nil {
		return nil, fmt.Errorf("malformed e2ee media reference: %w", err)
	}
	var decoded e2eeMediaRef
	if err := json.Unmarshal(raw, &decoded); err != nil {
		return nil, fmt.Errorf("malformed e2ee media reference: %w", err)
	}
	if !decoded.E2EE || decoded.DirectPath == "" || len(decoded.MediaKey) == 0 {
		return nil, fmt.Errorf("incomplete e2ee media reference")
	}
	return &decoded, nil
}

// e2eeClient returns the connected WhatsApp client or NotSupported when E2EE never came up.
func (m *Messenger) e2eeClient() (*whatsmeow.Client, *WireError) {
	deadline := time.Now().Add(10 * time.Second)
	for {
		m.e2eeMu.Lock()
		client := m.e2ee
		m.e2eeMu.Unlock()
		if client != nil {
			return client, nil
		}
		if time.Now().After(deadline) {
			return nil, errNotSupported("e2ee")
		}
		time.Sleep(100 * time.Millisecond)
	}
}

func (m *Messenger) e2eeStoreDir() string {
	m.mu.Lock()
	user := m.remoteUser
	m.mu.Unlock()
	return filepath.Join(m.dataDir, user)
}

// startE2EE opens the per-account WhatsApp device store and connects the E2EE socket. A fresh
// account registers a device with the ARMADILLO/ICDC endpoint first.
func (m *Messenger) startE2EE(ctx context.Context) error {
	m.e2eeMu.Lock()
	already := m.e2ee != nil
	m.e2eeMu.Unlock()
	if already {
		return nil
	}
	m.mu.Lock()
	user := m.remoteUser
	ownID := m.ownID
	cli := m.cli
	m.mu.Unlock()
	if user == "" || cli == nil {
		return fmt.Errorf("no session to attach the E2EE client to")
	}

	dir := m.e2eeStoreDir()
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return fmt.Errorf("create e2ee store directory: %w", err)
	}
	dbPath := filepath.ToSlash(filepath.Join(dir, "e2ee.db"))
	waLogger := waLog.Zerolog(m.log.With().Str("component", "whatsmeow").Logger())
	container, err := sqlstore.New(ctx, "sqlite",
		"file:"+dbPath+"?_pragma=foreign_keys(1)&_pragma=busy_timeout(5000)", waLogger)
	if err != nil {
		return fmt.Errorf("open e2ee store: %w", err)
	}
	device, err := container.GetFirstDevice(ctx)
	if err != nil {
		_ = container.Close()
		return fmt.Errorf("read e2ee device: %w", err)
	}
	// GetFirstDevice returns an unsaved device when the store is empty.
	isNew := device.ID == nil
	cli.SetDevice(device)
	if isNew {
		if err := cli.RegisterE2EE(ctx, ownID); err != nil {
			_ = container.Close()
			return fmt.Errorf("register e2ee device: %w", err)
		}
		if err := device.Save(ctx); err != nil {
			_ = container.Close()
			return fmt.Errorf("save e2ee device: %w", err)
		}
	}

	e2ee, err := cli.PrepareE2EEClient()
	if err != nil {
		_ = container.Close()
		return fmt.Errorf("prepare e2ee client: %w", err)
	}
	e2ee.AddEventHandlerWithSuccessStatus(m.handleE2EEEvent)
	// Retry receipts for outgoing messages are answered by whatsmeow itself: it looks the original
	// plaintext up, re-encrypts it for the device that failed to decrypt and re-sends the frame
	// (retry.go handleRetryReceipt). The in-memory ring buffer only covers the current process, so
	// persist the payloads in the device store as well: this sidecar reconnects constantly, and a
	// retry for a frame sent before the last restart still has to be answerable.
	if e2ee.Store != nil && e2ee.Store.EventBuffer != nil {
		e2ee.UseRetryMessageStore = true
	}

	m.e2eeMu.Lock()
	m.e2ee = e2ee
	m.deviceStore = container
	m.device = device
	m.e2eeMu.Unlock()

	if err := e2ee.Connect(); err != nil {
		return fmt.Errorf("connect e2ee socket: %w", err)
	}
	return nil
}

func (m *Messenger) handleE2EEEvent(raw any) bool {
	ctx := m.backgroundCtx()
	switch evt := raw.(type) {
	case *events.FBMessage:
		m.handleFBMessage(ctx, evt)
	case *events.Receipt:
		m.handleE2EEReceipt(ctx, evt)
	case *events.ChatPresence:
		m.handleE2EETyping(evt)
	case *events.UndecryptableMessage:
		m.handleE2EEUndecryptable(evt)
	case *events.Connected:
		m.log.Debug().Msg("E2EE socket connected")
		m.e2eeMu.Lock()
		m.e2eeAttempts = 0
		client := m.e2ee
		m.e2eeMu.Unlock()
		if client != nil {
			if err := client.SendPresence(ctx, waTypes.PresenceAvailable); err != nil {
				m.log.Debug().Err(err).Msg("Failed to mark the E2EE client available")
			}
			m.topUpE2EEPreKeys(ctx, client)
		}
	case *events.Disconnected:
		m.scheduleE2EEReconnect("socket closed")
	case *events.LoggedOut:
		m.log.Warn().Msg("E2EE session logged out; encrypted chats need a fresh login")
		m.cancelE2EERetry()
	case *events.ConnectFailure:
		m.scheduleE2EEReconnect(evt.Reason.String())
	default:
		_ = evt
	}
	return true
}

// e2eeBackoffLimit caps the wait between E2EE reconnect attempts. A socket that stays down is
// retried every minute, not in a tight loop.
const e2eeBackoffLimit = 60 * time.Second

// e2eeBackoff is the wait before the given 1-based reconnect attempt: one second, doubling, capped
// at e2eeBackoffLimit.
func e2eeBackoff(attempt int) time.Duration {
	wait := time.Second
	for i := 1; i < attempt && wait < e2eeBackoffLimit; i++ {
		wait *= 2
	}
	if wait > e2eeBackoffLimit {
		return e2eeBackoffLimit
	}
	return wait
}

// scheduleE2EEReconnect arms the next E2EE reconnect, replacing any pending one, so one outage
// keeps exactly one timer and a socket that keeps failing backs off instead of spinning. It warns
// once per outage and drops to debug for the repeats.
func (m *Messenger) scheduleE2EEReconnect(reason string) {
	ctx := m.backgroundCtx()
	if ctx == nil || ctx.Err() != nil {
		return
	}
	m.e2eeMu.Lock()
	if m.e2ee == nil {
		// Logged out or torn down: only a fresh login can bring the E2EE client back.
		m.e2eeMu.Unlock()
		return
	}
	m.e2eeAttempts++
	attempt := m.e2eeAttempts
	if m.e2eeRetry != nil {
		m.e2eeRetry.Stop()
	}
	wait := e2eeBackoff(attempt)
	var timer *time.Timer
	timer = time.AfterFunc(wait, func() {
		m.e2eeMu.Lock()
		current := m.e2eeRetry == timer
		if current {
			m.e2eeRetry = nil
		}
		m.e2eeMu.Unlock()
		if !current {
			// Superseded by a newer schedule or cancelled by a logout.
			return
		}
		m.reconnectE2EE(ctx, attempt)
	})
	m.e2eeRetry = timer
	m.e2eeMu.Unlock()

	event := m.log.Debug()
	if attempt == 1 {
		event = m.log.Warn()
	}
	event.Str("reason", reason).Int("attempt", attempt).Dur("retry_in", wait).
		Msg("Reconnecting the E2EE socket")
}

// cancelE2EERetry drops the pending reconnect and resets the attempt count; called on logout and on
// teardown, where the session needs a fresh login rather than another attempt.
func (m *Messenger) cancelE2EERetry() {
	m.e2eeMu.Lock()
	defer m.e2eeMu.Unlock()
	m.cancelE2EERetryLocked()
}

func (m *Messenger) cancelE2EERetryLocked() {
	if m.e2eeRetry != nil {
		m.e2eeRetry.Stop()
		m.e2eeRetry = nil
	}
	m.e2eeAttempts = 0
}

// reconnectE2EE opens the E2EE socket again through the same call startE2EE uses. A failed attempt
// schedules the next one; a socket that is already up counts as success.
func (m *Messenger) reconnectE2EE(ctx context.Context, attempt int) {
	if ctx.Err() != nil {
		return
	}
	m.e2eeMu.Lock()
	client := m.e2ee
	if client == nil || m.e2eeBusy {
		// Torn down or an attempt is already in flight; the pending timer covers the retry.
		m.e2eeMu.Unlock()
		return
	}
	m.e2eeBusy = true
	m.e2eeMu.Unlock()

	if client.IsConnected() {
		// whatsmeow reconnected on its own while the timer ran.
		m.e2eeMu.Lock()
		m.e2eeBusy = false
		m.e2eeAttempts = 0
		m.e2eeMu.Unlock()
		return
	}

	err := client.Connect()

	m.e2eeMu.Lock()
	m.e2eeBusy = false
	m.e2eeMu.Unlock()

	switch {
	case err == nil && client.IsConnected():
		return
	case errors.Is(err, whatsmeow.ErrAlreadyConnected):
		return
	case err != nil:
		m.log.Debug().Err(err).Int("attempt", attempt).Msg("E2EE reconnect attempt failed")
		m.scheduleE2EEReconnect(err.Error())
	default:
		// Connect accepted the attempt but handed the reconnect to its own background loop, and the
		// socket is not up yet: keep this connector's retry armed so encrypted chats come back even
		// if that loop stops (it does nothing when the device has no saved WhatsApp id).
		m.scheduleE2EEReconnect("socket still down")
	}
}

// e2eePreKeyTarget is the number of prekeys kept on the WhatsApp server. whatsmeow only uploads a
// batch of its own when the server count drops under MinPreKeyCount, so a server that is merely
// running low is never refilled; senders starting new sessions read one of those keys each time.
const e2eePreKeyTarget = whatsmeow.WantedPreKeyCount

// e2eePreKeyTimeout bounds the prekey reconciliation IQs so a stalled server never holds up the
// connect event handler.
const e2eePreKeyTimeout = 15 * time.Second

// needsE2EEPreKeyTopUp reports whether the server holds fewer prekeys than e2eePreKeyTarget.
func needsE2EEPreKeyTopUp(serverCount int) bool {
	return serverCount < e2eePreKeyTarget
}

// topUpE2EEPreKeys reconciles the device prekey store with the server after a connect: it reports
// the drift between the uploaded keys and the keys the server still holds, and uploads a fresh
// batch while the server is below e2eePreKeyTarget. The upload itself reuses the run context; a
// timeout keeps the two IQs bounded.
func (m *Messenger) topUpE2EEPreKeys(ctx context.Context, client *whatsmeow.Client) {
	if ctx == nil || client == nil || client.Store == nil || client.Store.PreKeys == nil || !client.IsConnected() {
		return
	}
	uploadCtx, cancel := context.WithTimeout(ctx, e2eePreKeyTimeout)
	defer cancel()

	localCount, err := client.Store.PreKeys.UploadedPreKeyCount(uploadCtx)
	if err != nil {
		m.log.Warn().Err(err).Msg("Failed to read the local WhatsApp prekey count")
		return
	}
	internals := client.DangerousInternals()
	serverCount, err := internals.GetServerPreKeyCount(uploadCtx)
	if err != nil {
		m.log.Warn().Err(err).Msg("Failed to read the WhatsApp prekey count on the server")
		return
	}
	m.log.Debug().Int("uploaded", localCount).Int("server", serverCount).
		Msg("E2EE prekey counts")
	if !needsE2EEPreKeyTopUp(serverCount) {
		return
	}

	internals.UploadPreKeys(uploadCtx, false)
	// UploadPreKeys reports a failed IQ only through its own logger, so read the count back: a
	// server count that did not move means the batch never landed and new sessions will fail.
	after, err := internals.GetServerPreKeyCount(uploadCtx)
	if err != nil {
		m.log.Warn().Err(err).Msg("Failed to read the WhatsApp prekey count after an upload")
		return
	}
	if after <= serverCount {
		m.log.Warn().Int("before", serverCount).Int("after", after).Int("uploaded", localCount).
			Msg("WhatsApp prekey upload did not increase the server count")
		return
	}
	m.log.Info().Int("before", serverCount).Int("after", after).Msg("Topped up WhatsApp prekeys")
}

// ensureE2EEChat makes sure the host knows the chat an encrypted event belongs to.
func (m *Messenger) ensureE2EEChat(jid waTypes.JID, isGroup bool, titleHint string) string {
	chatID := "e2ee:" + jid.String()
	m.mu.Lock()
	if _, ok := m.e2eeThreads[chatID]; ok {
		m.mu.Unlock()
		return chatID
	}
	for _, t := range m.threads {
		if t.chatID() == chatID {
			m.mu.Unlock()
			return chatID
		}
	}
	// Table rows for encrypted chats carry only the numeric thread key; adopt the real JID
	// once a message reveals it so the chat id matches the network.
	for _, t := range m.threads {
		if !t.Type.IsWhatsApp() || t.Key != int64(jid.UserInt()) {
			continue
		}
		oldID := t.chatID()
		t.JID = jid
		if oldID != chatID {
			chat := m.chatForThreadLocked(t)
			m.mu.Unlock()
			m.emit(NewChatRemovedEvent(oldID))
			m.emit(NewChatUpsertedEvent(chat))
			return chatID
		}
		m.mu.Unlock()
		return chatID
	}
	threadType := table.ENCRYPTED_OVER_WA_ONE_TO_ONE
	if isGroup || jid.Server == waTypes.GroupServer {
		threadType = table.ENCRYPTED_OVER_WA_GROUP
	}
	title := titleHint
	if title == "" {
		title = jid.User
	}
	t := &threadInfo{
		Key:            int64(jid.UserInt()),
		JID:            jid,
		Type:           threadType,
		Name:           title,
		LastActivityMs: time.Now().UnixMilli(),
	}
	m.e2eeThreads[chatID] = t
	chat := m.chatForThreadLocked(t)
	m.mu.Unlock()
	m.emit(NewChatUpsertedEvent(chat))
	return chatID
}

func (m *Messenger) handleFBMessage(ctx context.Context, evt *events.FBMessage) {
	consumer := evt.GetConsumerApplication()
	if consumer == nil {
		return
	}
	chatJID := evt.Info.Chat
	title := evt.Info.PushName
	chatID := m.ensureE2EEChat(chatJID, evt.Info.IsGroup, title)
	payload := consumer.GetPayload()
	if payload == nil {
		return
	}
	if content := payload.GetContent(); content != nil {
		var replyTo *string
		if quoted := evt.FBApplication.GetMetadata().GetQuotedMessage(); quoted.GetStanzaID() != "" {
			replyTo = new(quoted.GetStanzaID())
		}
		m.handleE2EEContent(ctx, chatID, chatJID, evt, content, replyTo)
	}
	if appData := payload.GetApplicationData(); appData != nil {
		if revoke := appData.GetRevoke(); revoke != nil && revoke.GetKey() != nil {
			msgID := revoke.GetKey().GetID()
			m.mu.Lock()
			delete(m.messages, msgID)
			m.mu.Unlock()
			m.emit(NewMessageDeletedEvent(chatID, msgID))
		}
	}
}

func (m *Messenger) handleE2EEContent(ctx context.Context, chatID string, chatJID waTypes.JID, evt *events.FBMessage, content *waConsumerApplication.ConsumerApplication_Content, replyTo *string) {
	senderJID := evt.Info.Sender.ToNonAD()
	senderID := strconv.FormatUint(senderJID.UserInt(), 10)
	sentAt := evt.Info.Timestamp.UnixMilli()
	var txn *string
	m.mu.Lock()
	if txnID, ok := m.txns[string(evt.Info.ID)]; ok {
		txn = &txnID
	}
	m.mu.Unlock()

	emit := func(msg RemoteMessage, sender waTypes.JID) {
		if msg.ReplyToRemoteMsgID == nil {
			msg.ReplyToRemoteMsgID = replyTo
		}
		m.mu.Lock()
		msg.Sender = m.profileLocked(int64(sender.UserInt()))
		m.rememberMessageLocked(msg.RemoteMsgID, &msgMeta{
			ThreadKey:   -1,
			ChatID:      msg.RemoteChatID,
			SenderID:    int64(sender.UserInt()),
			SenderJID:   sender,
			IsOwn:       msg.IsOwn,
			TimestampMs: msg.SentAt,
			Text:        deref(msg.Body),
		})
		m.mu.Unlock()
		m.emit(NewMessageEvent(msg))
	}

	switch {
	case content.GetMessageText() != nil:
		text := content.GetMessageText().GetText()
		emit(RemoteMessage{
			RemoteChatID:   chatID,
			RemoteMsgID:    string(evt.Info.ID),
			TxnID:          txn,
			SenderRemoteID: senderID,
			IsOwn:          evt.Info.IsFromMe,
			SentAt:         sentAt,
			Kind:           "text",
			Body:           &text,
		}, senderJID)
	case content.GetExtendedTextMessage() != nil:
		text := content.GetExtendedTextMessage().GetText().GetText()
		emit(RemoteMessage{
			RemoteChatID:   chatID,
			RemoteMsgID:    string(evt.Info.ID),
			TxnID:          txn,
			SenderRemoteID: senderID,
			IsOwn:          evt.Info.IsFromMe,
			SentAt:         sentAt,
			Kind:           "text",
			Body:           &text,
		}, senderJID)
	case content.GetImageMessage() != nil:
		msg := content.GetImageMessage()
		kind, ref, caption := "image", m.imageRef(msg), msg.GetCaption().GetText()
		m.emitE2EEMedia(chatID, chatJID, evt, senderID, senderJID, sentAt, txn, kind, ref, caption, replyTo)
	case content.GetVideoMessage() != nil:
		msg := content.GetVideoMessage()
		kind, ref, caption := "video", m.videoRef(msg), msg.GetCaption().GetText()
		m.emitE2EEMedia(chatID, chatJID, evt, senderID, senderJID, sentAt, txn, kind, ref, caption, replyTo)
	case content.GetAudioMessage() != nil:
		msg := content.GetAudioMessage()
		kind := "audio"
		if msg.GetPTT() {
			kind = "voice"
		}
		m.emitE2EEMedia(chatID, chatJID, evt, senderID, senderJID, sentAt, txn, kind, m.audioRef(msg), "", replyTo)
	case content.GetDocumentMessage() != nil:
		msg := content.GetDocumentMessage()
		m.emitE2EEMedia(chatID, chatJID, evt, senderID, senderJID, sentAt, txn, "file", m.documentRef(msg), "", replyTo)
	case content.GetStickerMessage() != nil:
		m.emitE2EEMedia(chatID, chatJID, evt, senderID, senderJID, sentAt, txn, "sticker", m.stickerRef(content.GetStickerMessage()), "", replyTo)
	case content.GetEditMessage() != nil:
		edit := content.GetEditMessage()
		if edit.GetKey() == nil {
			return
		}
		editedAt := edit.GetTimestampMS()
		if editedAt == 0 {
			editedAt = sentAt
		}
		body := edit.GetMessage().GetText()
		m.mu.Lock()
		if meta := m.messages[edit.GetKey().GetID()]; meta != nil {
			meta.Text = body
		}
		m.mu.Unlock()
		m.emit(NewMessageEditedEvent(chatID, edit.GetKey().GetID(), &body, editedAt))
	case content.GetReactionMessage() != nil:
		reaction := content.GetReactionMessage()
		if reaction.GetKey() == nil {
			return
		}
		emoji := reaction.GetText()
		m.emit(NewReactionEvent(chatID, reaction.GetKey().GetID(), senderID, emoji, emoji != ""))
	}
}

func (m *Messenger) emitE2EEMedia(chatID string, chatJID waTypes.JID, evt *events.FBMessage, senderID string, senderJID waTypes.JID, sentAt int64, txn *string, kind string, ref *MediaRef, caption string, replyTo *string) {
	msg := RemoteMessage{
		RemoteChatID:       chatID,
		RemoteMsgID:        string(evt.Info.ID),
		TxnID:              txn,
		SenderRemoteID:     senderID,
		IsOwn:              evt.Info.IsFromMe,
		SentAt:             sentAt,
		Kind:               kind,
		ReplyToRemoteMsgID: replyTo,
		Media:              ref,
	}
	if caption != "" {
		msg.Body = &caption
	}
	m.mu.Lock()
	msg.Sender = m.profileLocked(int64(senderJID.UserInt()))
	m.rememberMessageLocked(msg.RemoteMsgID, &msgMeta{
		ThreadKey:   -1,
		ChatID:      chatID,
		SenderID:    int64(senderJID.UserInt()),
		SenderJID:   senderJID,
		IsOwn:       msg.IsOwn,
		TimestampMs: sentAt,
		Text:        caption,
	})
	m.mu.Unlock()
	m.emit(NewMessageEvent(msg))
	_ = chatJID
}

func (m *Messenger) imageRef(msg *waConsumerApplication.ConsumerApplication_ImageMessage) *MediaRef {
	transport, err := msg.Decode()
	if err != nil {
		return nil
	}
	return transportRef(whatsmeow.MediaImage, transport.Integral.GetTransport(), transport.Ancillary.GetWidth(), transport.Ancillary.GetHeight())
}

func (m *Messenger) videoRef(msg *waConsumerApplication.ConsumerApplication_VideoMessage) *MediaRef {
	transport, err := msg.Decode()
	if err != nil {
		return nil
	}
	return transportRef(whatsmeow.MediaVideo, transport.Integral.GetTransport(), transport.Ancillary.GetWidth(), transport.Ancillary.GetHeight())
}

func (m *Messenger) audioRef(msg *waConsumerApplication.ConsumerApplication_AudioMessage) *MediaRef {
	transport, err := msg.Decode()
	if err != nil {
		return nil
	}
	return transportRef(whatsmeow.MediaAudio, transport.Integral.GetTransport(), 0, 0)
}

func (m *Messenger) documentRef(msg *waConsumerApplication.ConsumerApplication_DocumentMessage) *MediaRef {
	transport, err := msg.Decode()
	if err != nil {
		return nil
	}
	return transportRef(whatsmeow.MediaDocument, transport.Integral.GetTransport(), 0, 0)
}

func (m *Messenger) stickerRef(msg *waConsumerApplication.ConsumerApplication_StickerMessage) *MediaRef {
	transport, err := msg.Decode()
	if err != nil {
		return nil
	}
	ref := transportRef(whatsmeow.MediaImage, transport.Integral.GetTransport(), transport.Ancillary.GetWidth(), transport.Ancillary.GetHeight())
	if ref != nil {
		ref.Mime = "image/webp"
	}
	return ref
}

func transportRef(mediaType whatsmeow.MediaType, transport *waMediaTransport.WAMediaTransport, width, height uint32) *MediaRef {
	if transport == nil || transport.Integral == nil {
		return nil
	}
	integral := transport.Integral
	ancillary := transport.Ancillary
	ref := &MediaRef{
		RemoteRef: encodeE2EEMediaRef(mediaType, integral.GetDirectPath(), integral.GetMediaKey(),
			integral.GetFileSHA256(), integral.GetFileEncSHA256()),
		Mime: "application/octet-stream",
	}
	if ancillary != nil {
		if ancillary.GetMimetype() != "" {
			ref.Mime = ancillary.GetMimetype()
		}
		if ancillary.GetFileLength() > 0 {
			ref.SizeBytes = new(ancillary.GetFileLength())
		}
	}
	if width > 0 {
		ref.Width = new(width)
	}
	if height > 0 {
		ref.Height = new(height)
	}
	return ref
}

// handleE2EEReceipt turns read receipts into host read markers. Retry receipts are answered by
// whatsmeow itself — retry.go handleRetryReceipt re-encrypts the original payload and re-sends the
// frame to the device that failed to decrypt — so the sidecar only surfaces them here: a retry the
// library cannot satisfy (no stored payload) is exactly what leaves a message undecrypted.
func (m *Messenger) handleE2EEReceipt(ctx context.Context, evt *events.Receipt) {
	if evt.Type == waTypes.ReceiptTypeRetry {
		ids := make([]string, 0, len(evt.MessageIDs))
		for _, id := range evt.MessageIDs {
			ids = append(ids, string(id))
		}
		event := m.log.Warn().
			Str("chat", evt.Chat.String()).
			Str("recipient", evt.Sender.String()).
			Strs("message_ids", ids)
		m.e2eeMu.Lock()
		client := m.e2ee
		m.e2eeMu.Unlock()
		if ctx != nil && client != nil && len(evt.MessageIDs) > 0 {
			msg, err := client.DangerousInternals().GetMessageForRetry(ctx, evt, evt.MessageIDs[0])
			switch {
			case err != nil:
				event = event.Err(err)
			case msg == nil || msg.IsEmpty():
				event = event.Bool("retry_payload_missing", true)
			}
		}
		event.Msg("WhatsApp asked us to re-send an outgoing message to another device")
		return
	}
	if evt.Type != waTypes.ReceiptTypeRead && evt.Type != waTypes.ReceiptTypeReadSelf {
		return
	}
	if len(evt.MessageIDs) == 0 {
		return
	}
	chatID := "e2ee:" + evt.Chat.String()
	user := evt.Sender.ToNonAD().User
	if user == "" {
		user = evt.Chat.User
	}
	m.emit(NewReadMarkerEvent(chatID, user, string(evt.MessageIDs[len(evt.MessageIDs)-1])))
}

// handleE2EEUndecryptable logs a message the E2EE layer could not open and returns so the sync loop
// keeps going. whatsmeow has already sent a retry receipt to the sender's devices and re-emits the
// message as a normal event once one of them re-sends it decryptable, so nothing is dropped here.
func (m *Messenger) handleE2EEUndecryptable(evt *events.UndecryptableMessage) {
	info := evt.Info
	event := m.log.Warn().
		Str("message_id", string(info.ID)).
		Str("chat", info.Chat.String()).
		Str("sender", info.Sender.String()).
		Bool("unavailable", evt.IsUnavailable).
		Str("decrypt_fail_mode", string(evt.DecryptFailMode))
	if evt.UnavailableType != events.UnavailableTypeUnknown {
		event = event.Str("unavailable_type", string(evt.UnavailableType))
	}
	event.Msg("Failed to decrypt a WhatsApp message; a retry receipt was sent to the sender")
}

func (m *Messenger) handleE2EETyping(evt *events.ChatPresence) {
	chatID := "e2ee:" + evt.Chat.String()
	m.emit(NewTypingEvent(chatID, evt.Sender.ToNonAD().User, evt.State == waTypes.ChatPresenceComposing))
}

func (m *Messenger) sendE2EE(ctx context.Context, cli *messagix.Client, msg OutgoingMessage, jid waTypes.JID) (any, *WireError) {
	client, werr := m.e2eeClient()
	if werr != nil {
		return nil, werr
	}
	otid := methods.GenerateEpochID()
	id := strconv.FormatInt(otid, 10)

	var content *waConsumerApplication.ConsumerApplication_Content
	if msg.Attachment != nil {
		var buildErr *WireError
		content, buildErr = m.buildE2EEMediaContent(ctx, client, jid, msg)
		if buildErr != nil {
			return nil, buildErr
		}
	} else {
		body := deref(msg.Body)
		content = &waConsumerApplication.ConsumerApplication_Content{
			Content: &waConsumerApplication.ConsumerApplication_Content_MessageText{
				MessageText: &waCommon.MessageText{Text: &body},
			},
		}
	}
	app := &waConsumerApplication.ConsumerApplication{
		Payload: &waConsumerApplication.ConsumerApplication_Payload{
			Payload: &waConsumerApplication.ConsumerApplication_Payload_Content{Content: content},
		},
	}
	var metadata *waMsgApplication.MessageApplication_Metadata
	if msg.ReplyTo != nil {
		metadata = &waMsgApplication.MessageApplication_Metadata{
			QuotedMessage: &waMsgApplication.MessageApplication_Metadata_QuotedMessage{
				StanzaID:    new(*msg.ReplyTo),
				Participant: new(jid.String()),
			},
		}
	}
	m.mu.Lock()
	m.txns[id] = msg.TxnID
	m.mu.Unlock()
	resp, err := client.SendFBMessage(ctx, jid, app, metadata, whatsmeow.SendRequestExtra{ID: id})
	if err != nil {
		return nil, mapE2EEError(err)
	}
	sentAt := resp.Timestamp.UnixMilli()
	if sentAt == 0 {
		sentAt = time.Now().UnixMilli()
	}
	chatID := "e2ee:" + jid.String()
	m.e2eeMu.Lock()
	device := m.device
	m.e2eeMu.Unlock()
	senderJID := waTypes.EmptyJID
	if device != nil && device.ID != nil {
		senderJID = device.ID.ToNonAD()
	}
	m.mu.Lock()
	m.rememberMessageLocked(id, &msgMeta{
		ChatID:      chatID,
		SenderID:    m.ownID,
		SenderJID:   senderJID,
		IsOwn:       true,
		TimestampMs: sentAt,
		Text:        deref(msg.Body),
	})
	m.mu.Unlock()
	_ = cli
	return SendAck{TxnID: msg.TxnID, RemoteMsgID: id, SentAt: sentAt}, nil
}

func (m *Messenger) buildE2EEMediaContent(ctx context.Context, client *whatsmeow.Client, jid waTypes.JID, msg OutgoingMessage) (*waConsumerApplication.ConsumerApplication_Content, *WireError) {
	att := msg.Attachment
	info, err := os.Stat(att.Path)
	if err != nil {
		return nil, errInvalidInput(fmt.Sprintf("attachment %q: %v", att.Path, err))
	}
	if !info.Mode().IsRegular() {
		return nil, errInvalidInput(fmt.Sprintf("attachment %q is not a regular file", att.Path))
	}
	if info.Size() > maxUploadBytes {
		return nil, errInvalidInput(fmt.Sprintf("attachment is %d bytes; the network accepts at most %d", info.Size(), maxUploadBytes))
	}
	data, err := os.ReadFile(att.Path)
	if err != nil {
		return nil, errInvalidInput(fmt.Sprintf("attachment %q: %v", att.Path, err))
	}
	mime := ""
	if att.Mime != nil {
		mime = *att.Mime
	}
	if mime == "" {
		mime = detectMime(data)
	}
	name := filepath.Base(att.Path)
	if att.FileName != nil && *att.FileName != "" {
		name = *att.FileName
	}
	caption := deref(msg.Body)

	mediaType := whatsmeow.MediaDocument
	kind := "file"
	switch {
	case len(mime) >= 6 && mime[:6] == "image/":
		mediaType, kind = whatsmeow.MediaImage, "image"
	case len(mime) >= 6 && mime[:6] == "video/":
		mediaType, kind = whatsmeow.MediaVideo, "video"
	case len(mime) >= 6 && mime[:6] == "audio/":
		mediaType, kind = whatsmeow.MediaAudio, "audio"
	}
	uploaded, err := client.Upload(ctx, data, mediaType)
	if err != nil {
		return nil, mapE2EEError(err)
	}
	width, height := mediaDimensions(data, kind)
	transport := &waMediaTransport.WAMediaTransport{
		Integral: &waMediaTransport.WAMediaTransport_Integral{
			FileSHA256:        uploaded.FileSHA256,
			MediaKey:          uploaded.MediaKey,
			FileEncSHA256:     uploaded.FileEncSHA256,
			DirectPath:        &uploaded.DirectPath,
			MediaKeyTimestamp: new(time.Now().Unix()),
		},
		Ancillary: &waMediaTransport.WAMediaTransport_Ancillary{
			FileLength: new(uint64(len(data))),
			Mimetype:   &mime,
			Thumbnail: &waMediaTransport.WAMediaTransport_Ancillary_Thumbnail{
				ThumbnailWidth:  new(width),
				ThumbnailHeight: new(height),
			},
			ObjectID: &uploaded.ObjectID,
		},
	}
	var content *waConsumerApplication.ConsumerApplication_Content
	switch kind {
	case "image":
		imageMsg := &waConsumerApplication.ConsumerApplication_ImageMessage{Caption: textOrNil(caption)}
		if err := imageMsg.Set(&waMediaTransport.ImageTransport{
			Integral:  &waMediaTransport.ImageTransport_Integral{Transport: transport},
			Ancillary: &waMediaTransport.ImageTransport_Ancillary{Width: new(width), Height: new(height)},
		}); err != nil {
			return nil, errProtocol("encode image transport: " + err.Error())
		}
		content = &waConsumerApplication.ConsumerApplication_Content{Content: &waConsumerApplication.ConsumerApplication_Content_ImageMessage{ImageMessage: imageMsg}}
	case "video":
		videoMsg := &waConsumerApplication.ConsumerApplication_VideoMessage{Caption: textOrNil(caption)}
		if err := videoMsg.Set(&waMediaTransport.VideoTransport{
			Integral:  &waMediaTransport.VideoTransport_Integral{Transport: transport},
			Ancillary: &waMediaTransport.VideoTransport_Ancillary{Width: new(width), Height: new(height)},
		}); err != nil {
			return nil, errProtocol("encode video transport: " + err.Error())
		}
		content = &waConsumerApplication.ConsumerApplication_Content{Content: &waConsumerApplication.ConsumerApplication_Content_VideoMessage{VideoMessage: videoMsg}}
	case "audio":
		audioMsg := &waConsumerApplication.ConsumerApplication_AudioMessage{}
		if err := audioMsg.Set(&waMediaTransport.AudioTransport{
			Integral:  &waMediaTransport.AudioTransport_Integral{Transport: transport},
			Ancillary: &waMediaTransport.AudioTransport_Ancillary{},
		}); err != nil {
			return nil, errProtocol("encode audio transport: " + err.Error())
		}
		content = &waConsumerApplication.ConsumerApplication_Content{Content: &waConsumerApplication.ConsumerApplication_Content_AudioMessage{AudioMessage: audioMsg}}
	default:
		documentMsg := &waConsumerApplication.ConsumerApplication_DocumentMessage{FileName: &name}
		if err := documentMsg.Set(&waMediaTransport.DocumentTransport{
			Integral:  &waMediaTransport.DocumentTransport_Integral{Transport: transport},
			Ancillary: &waMediaTransport.DocumentTransport_Ancillary{},
		}); err != nil {
			return nil, errProtocol("encode document transport: " + err.Error())
		}
		content = &waConsumerApplication.ConsumerApplication_Content{Content: &waConsumerApplication.ConsumerApplication_Content_DocumentMessage{DocumentMessage: documentMsg}}
	}
	_ = jid
	return content, nil
}

func detectMime(data []byte) string {
	trimmed := data
	if len(trimmed) > 512 {
		trimmed = trimmed[:512]
	}
	return http.DetectContentType(trimmed)
}

// mediaDimensions returns the thumbnail box sent alongside media. Images are measured; other
// kinds use a 1:1 placeholder, which the client needs only to be non-empty.
func mediaDimensions(data []byte, kind string) (uint32, uint32) {
	if kind == "image" {
		if cfg, _, err := image.DecodeConfig(bytes.NewReader(data)); err == nil && cfg.Width > 0 && cfg.Height > 0 {
			w, h := cfg.Width, cfg.Height
			if w > 400 {
				h = h * 400 / w
				w = 400
			}
			if h > 400 {
				w = w * 400 / h
				h = 400
			}
			return uint32(w), uint32(h)
		}
		return 400, 400
	}
	return 1, 1
}

func textOrNil(text string) *waCommon.MessageText {
	return &waCommon.MessageText{Text: &text}
}

func (m *Messenger) editE2EE(ctx context.Context, cli *messagix.Client, jid waTypes.JID, p EditParams) (any, *WireError) {
	client, werr := m.e2eeClient()
	if werr != nil {
		return nil, werr
	}
	edit := &waConsumerApplication.ConsumerApplication_EditMessage{
		Key:         m.waMessageKey(jid, p.RemoteMsgID, true),
		Message:     textOrNil(p.Body),
		TimestampMS: new(time.Now().UnixMilli()),
	}
	app := wrapConsumerContent(&waConsumerApplication.ConsumerApplication_Content{
		Content: &waConsumerApplication.ConsumerApplication_Content_EditMessage{EditMessage: edit},
	})
	if _, err := client.SendFBMessage(ctx, jid, app, nil, whatsmeow.SendRequestExtra{ID: strconv.FormatInt(methods.GenerateEpochID(), 10)}); err != nil {
		return nil, mapE2EEError(err)
	}
	_ = cli
	return nil, nil
}

func (m *Messenger) deleteE2EE(ctx context.Context, cli *messagix.Client, jid waTypes.JID, p DeleteParams) (any, *WireError) {
	client, werr := m.e2eeClient()
	if werr != nil {
		return nil, werr
	}
	revoke := &waConsumerApplication.ConsumerApplication_RevokeMessage{
		Key: m.waMessageKey(jid, p.RemoteMsgID, true),
	}
	app := &waConsumerApplication.ConsumerApplication{
		Payload: &waConsumerApplication.ConsumerApplication_Payload{
			Payload: &waConsumerApplication.ConsumerApplication_Payload_ApplicationData{
				ApplicationData: &waConsumerApplication.ConsumerApplication_ApplicationData{
					ApplicationContent: &waConsumerApplication.ConsumerApplication_ApplicationData_Revoke{Revoke: revoke},
				},
			},
		},
	}
	if _, err := client.SendFBMessage(ctx, jid, app, nil, whatsmeow.SendRequestExtra{ID: strconv.FormatInt(methods.GenerateEpochID(), 10)}); err != nil {
		return nil, mapE2EEError(err)
	}
	m.mu.Lock()
	delete(m.messages, p.RemoteMsgID)
	m.mu.Unlock()
	_ = cli
	return nil, nil
}

func (m *Messenger) reactE2EE(ctx context.Context, cli *messagix.Client, jid waTypes.JID, p ReactParams, add bool) (any, *WireError) {
	client, werr := m.e2eeClient()
	if werr != nil {
		return nil, werr
	}
	emoji := ""
	if add {
		emoji = p.Emoji
	}
	reaction := &waConsumerApplication.ConsumerApplication_ReactionMessage{
		Key:               m.waMessageKey(jid, p.RemoteMsgID, false),
		Text:              &emoji,
		SenderTimestampMS: new(time.Now().UnixMilli()),
	}
	app := wrapConsumerContent(&waConsumerApplication.ConsumerApplication_Content{
		Content: &waConsumerApplication.ConsumerApplication_Content_ReactionMessage{ReactionMessage: reaction},
	})
	if _, err := client.SendFBMessage(ctx, jid, app, nil, whatsmeow.SendRequestExtra{ID: strconv.FormatInt(methods.GenerateEpochID(), 10)}); err != nil {
		return nil, mapE2EEError(err)
	}
	_ = cli
	return nil, nil
}

func (m *Messenger) markReadE2EE(ctx context.Context, cli *messagix.Client, jid waTypes.JID, p MarkReadParams) (any, *WireError) {
	client, werr := m.e2eeClient()
	if werr != nil {
		return nil, werr
	}
	if err := client.MarkRead(ctx, []waTypes.MessageID{waTypes.MessageID(p.UpToRemoteMsgID)}, time.Now(), jid, waTypes.EmptyJID); err != nil {
		return nil, mapE2EEError(err)
	}
	_ = cli
	return nil, nil
}

func (m *Messenger) setTypingE2EE(ctx context.Context, cli *messagix.Client, jid waTypes.JID, typing bool) (any, *WireError) {
	client, werr := m.e2eeClient()
	if werr != nil {
		return nil, werr
	}
	state := waTypes.ChatPresencePaused
	if typing {
		state = waTypes.ChatPresenceComposing
		if err := client.SendPresence(ctx, waTypes.PresenceAvailable); err != nil {
			m.log.Debug().Err(err).Msg("Failed to mark the E2EE client available before typing")
		}
	}
	if err := client.SendChatPresence(ctx, jid, state, waTypes.ChatPresenceMediaText); err != nil {
		return nil, mapE2EEError(err)
	}
	_ = cli
	return nil, nil
}

// fetchE2EEMedia decrypts and writes one encrypted attachment.
func (m *Messenger) fetchE2EEMedia(ctx context.Context, ref *e2eeMediaRef, destPath string) (uint64, *WireError) {
	client, werr := m.e2eeClient()
	if werr != nil {
		return 0, werr
	}
	data, err := client.DownloadMediaWithPath(ctx, ref.DirectPath, ref.FileEncSHA256, ref.FileSHA256,
		ref.MediaKey, whatsmeow.MediaType(ref.MediaType), "", false)
	if err != nil {
		return 0, mapE2EEError(err)
	}
	if err := writeMediaFile(destPath, data); err != nil {
		return 0, err
	}
	return uint64(len(data)), nil
}

// waMessageKey builds the WhatsApp message key used for edits, revokes and reactions.
func (m *Messenger) waMessageKey(chatJID waTypes.JID, msgID string, forceOwn bool) *waCommon.MessageKey {
	key := &waCommon.MessageKey{
		RemoteJID: new(chatJID.String()),
		ID:        new(msgID),
	}
	m.mu.Lock()
	meta := m.messages[msgID]
	m.mu.Unlock()
	if meta == nil {
		if forceOwn || chatJID.Server == waTypes.MessengerServer || chatJID.Server == waTypes.DefaultUserServer {
			key.FromMe = new(forceOwn)
		}
		return key
	}
	if meta.IsOwn || forceOwn {
		key.FromMe = new(true)
		return key
	}
	key.FromMe = new(false)
	if chatJID.Server != waTypes.MessengerServer && chatJID.Server != waTypes.DefaultUserServer {
		key.Participant = new(meta.SenderJID.String())
	}
	return key
}

func wrapConsumerContent(content *waConsumerApplication.ConsumerApplication_Content) *waConsumerApplication.ConsumerApplication {
	return &waConsumerApplication.ConsumerApplication{
		Payload: &waConsumerApplication.ConsumerApplication_Payload{
			Payload: &waConsumerApplication.ConsumerApplication_Payload_Content{Content: content},
		},
	}
}

func mapE2EEError(err error) *WireError {
	if err == nil {
		return nil
	}
	if errors.Is(err, whatsmeow.ErrClientIsNil) {
		return errNotSupported("e2ee")
	}
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return errNetwork(err.Error())
	}
	return errNetwork(err.Error())
}

func deref(value *string) string {
	if value == nil {
		return ""
	}
	return *value
}
