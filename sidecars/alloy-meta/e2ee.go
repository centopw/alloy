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
		m.handleE2EEReceipt(evt)
	case *events.ChatPresence:
		m.handleE2EETyping(evt)
	case *events.Connected:
		m.log.Debug().Msg("E2EE socket connected")
		m.e2eeMu.Lock()
		client := m.e2ee
		m.e2eeMu.Unlock()
		if client != nil {
			if err := client.SendPresence(ctx, waTypes.PresenceAvailable); err != nil {
				m.log.Debug().Err(err).Msg("Failed to mark the E2EE client available")
			}
		}
	case *events.Disconnected:
		m.log.Warn().Msg("E2EE socket disconnected")
	case *events.LoggedOut:
		m.log.Warn().Msg("E2EE session logged out; encrypted chats need a fresh login")
	case *events.ConnectFailure:
		m.log.Warn().Str("reason", evt.Reason.String()).Msg("E2EE connection failed")
	default:
		_ = evt
	}
	return true
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

func (m *Messenger) handleE2EEReceipt(evt *events.Receipt) {
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
