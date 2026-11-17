package main

import (
	"errors"
	"sort"
	"strconv"

	"go.mau.fi/mautrix-meta/pkg/messagix/securehistory"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waArmadilloBackupMessage"
	"go.mau.fi/whatsmeow/proto/waConsumerApplication"
	"go.mau.fi/whatsmeow/proto/waMediaTransport"
	"go.mau.fi/whatsmeow/proto/waMsgApplication"
	waTypes "go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"
)

// archiveRows converts one decrypted archive page into connector messages, newest first, folding
// each message's supplements onto it and emitting the reactions the archive recorded. It mirrors
// the live E2EE mapping: same fields, same kinds, same profile lookup.
func (m *Messenger) archiveRows(messages []*securehistory.DecryptedMessage, chatID string, jid waTypes.JID) []RemoteMessage {
	rows := make([]RemoteMessage, 0, len(messages))
	var reactions []ReactionEvent
	for _, message := range messages {
		row, folded, ok := m.archiveRow(message, chatID, jid)
		if !ok {
			continue
		}
		rows = append(rows, row)
		reactions = append(reactions, folded...)
	}
	sort.SliceStable(rows, func(i, j int) bool {
		if rows[i].SentAt != rows[j].SentAt {
			return rows[i].SentAt > rows[j].SentAt
		}
		return rows[i].RemoteMsgID > rows[j].RemoteMsgID
	})
	for _, reaction := range reactions {
		m.emit(reaction)
	}
	return rows
}

// archiveRow maps one decrypted archive message. Nothing here writes crypto: the message is
// already plaintext, and the transport ancillary supplies the same mime/size/keys the live path
// uses; the archive attachment only replaces the download URL, matched by object id.
func (m *Messenger) archiveRow(message *securehistory.DecryptedMessage, chatID string, jid waTypes.JID) (RemoteMessage, []ReactionEvent, bool) {
	if message == nil || message.Message == nil || message.Message.Message == nil {
		return RemoteMessage{}, nil, false
	}
	parent := message.Message.Message
	metadata := parent.GetMetadata()
	if metadata.GetMessageID() == "" || metadata.GetSenderID() == "" {
		return RemoteMessage{}, nil, false
	}
	evt, err := decodeArchivedApplication(parent)
	if err != nil {
		m.log.Debug().Err(err).Str("archive_otid", message.ID).Msg("Skipping an archive message with no readable application")
		return RemoteMessage{}, nil, false
	}
	consumer := evt.GetConsumerApplication()
	if consumer == nil || consumer.GetPayload().GetContent() == nil {
		return RemoteMessage{}, nil, false
	}
	content := consumer.GetPayload().GetContent()
	sentAt := message.Message.TimestampMS
	if sentAt <= 0 {
		sentAt = metadata.GetTimestampMS()
	}
	senderID := metadata.GetSenderID()
	sender, _ := strconv.ParseInt(senderID, 10, 64)
	urls := archiveAttachmentURLs(message.Attachments)

	row := RemoteMessage{
		RemoteChatID:   chatID,
		RemoteMsgID:    metadata.GetMessageID(),
		SenderRemoteID: senderID,
		IsOwn:          sender != 0 && sender == m.ownID,
		SentAt:         sentAt,
	}
	if quoted := evt.FBApplication.GetMetadata().GetQuotedMessage().GetStanzaID(); quoted != "" {
		row.ReplyToRemoteMsgID = &quoted
	}
	if !fillArchiveContent(&row, content, urls) {
		m.log.Debug().Str("archive_otid", message.ID).Msg("Skipping an archive message this client does not map")
		return RemoteMessage{}, nil, false
	}
	reactions := foldArchiveSupplementals(&row, message.Supplementals, chatID)

	m.mu.Lock()
	if sender != 0 {
		row.Sender = m.profileLocked(sender)
	}
	m.rememberMessageLocked(row.RemoteMsgID, &msgMeta{
		ThreadKey:   -1,
		ChatID:      chatID,
		SenderID:    sender,
		SenderJID:   waTypes.JID{User: senderID, Server: waTypes.MessengerServer},
		IsOwn:       row.IsOwn,
		TimestampMs: sentAt,
		Text:        deref(row.Body),
	})
	m.mu.Unlock()
	return row, reactions, true
}

// fillArchiveContent applies the content oneof the same way the live path does. It reports false
// for content types this connector does not map, which the caller skips with a debug line.
func fillArchiveContent(row *RemoteMessage, content *waConsumerApplication.ConsumerApplication_Content, urls map[string]string) bool {
	switch {
	case content.GetMessageText() != nil:
		text := content.GetMessageText().GetText()
		row.Kind, row.Body = "text", &text
	case content.GetExtendedTextMessage() != nil:
		text := content.GetExtendedTextMessage().GetText().GetText()
		row.Kind, row.Body = "text", &text
	case content.GetImageMessage() != nil:
		message := content.GetImageMessage()
		row.Kind = "image"
		row.Media = archiveImageRef(message, urls)
		if caption := message.GetCaption().GetText(); caption != "" {
			row.Body = &caption
		}
	case content.GetVideoMessage() != nil:
		message := content.GetVideoMessage()
		row.Kind = "video"
		row.Media = archiveVideoRef(message, urls)
		if caption := message.GetCaption().GetText(); caption != "" {
			row.Body = &caption
		}
	case content.GetAudioMessage() != nil:
		message := content.GetAudioMessage()
		row.Kind = "audio"
		if message.GetPTT() {
			row.Kind = "voice"
		}
		row.Media = archiveAudioRef(message, urls)
	case content.GetDocumentMessage() != nil:
		row.Kind = "file"
		row.Media = archiveDocumentRef(content.GetDocumentMessage(), urls)
	case content.GetStickerMessage() != nil:
		row.Kind = "sticker"
		row.Media = archiveStickerRef(content.GetStickerMessage(), urls)
	default:
		return false
	}
	return true
}

// foldArchiveSupplementals applies edits to the body and returns the reactions the archive stored
// alongside the message. Reactions are emitted as events because the message shape has no reaction
// list; the host applies them to the parent exactly as it does for live reactions.
func foldArchiveSupplementals(row *RemoteMessage, supplementals map[string]*securehistory.DecryptedProtobuf, chatID string) []ReactionEvent {
	var reactions []ReactionEvent
	var editedAt int64
	for _, supplemental := range supplementals {
		if supplemental == nil || supplemental.Message == nil {
			continue
		}
		evt, err := decodeArchivedApplication(supplemental.Message)
		if err != nil || evt.GetConsumerApplication() == nil {
			continue
		}
		content := evt.GetConsumerApplication().GetPayload().GetContent()
		actor := supplemental.Message.GetMetadata().GetSenderID()
		switch {
		case content.GetEditMessage() != nil:
			edit := content.GetEditMessage()
			if target := edit.GetKey().GetID(); target != "" && target != row.RemoteMsgID {
				continue
			}
			if edit.GetMessage() == nil {
				continue
			}
			timestamp := supplemental.TimestampMS
			if timestamp <= 0 {
				timestamp = edit.GetTimestampMS()
			}
			if timestamp < editedAt {
				continue
			}
			body := edit.GetMessage().GetText()
			row.Body = &body
			editedAt = timestamp
			row.EditedAt = &editedAt
		case content.GetReactionMessage() != nil:
			reaction := content.GetReactionMessage()
			if target := reaction.GetKey().GetID(); target != "" && target != row.RemoteMsgID {
				continue
			}
			emoji := reaction.GetText()
			reactions = append(reactions, NewReactionEvent(chatID, row.RemoteMsgID, actor, emoji, emoji != ""))
		}
	}
	return reactions
}

// decodeArchivedApplication unwraps the archived transport stanza into the same event shape the
// live socket produces.
func decodeArchivedApplication(message *waArmadilloBackupMessage.BackupMessage) (*events.FBMessage, error) {
	payload, ok := message.GetPayload().(*waArmadilloBackupMessage.BackupMessage_EncryptedTransportMessage)
	if !ok {
		return nil, errUnsupportedArchivePayload
	}
	var application waMsgApplication.MessageApplication
	if err := proto.Unmarshal(payload.EncryptedTransportMessage, &application); err != nil {
		return nil, err
	}
	decoded, err := whatsmeow.DecodeFBApplication(&application)
	if err != nil {
		return nil, err
	}
	return &decoded, nil
}

// archiveAttachmentURLs indexes the page's attachments by object id so a transport ancillary can
// be pointed at the archive's CDN URL.
func archiveAttachmentURLs(attachments []securehistory.ArchiveAttachment) map[string]string {
	if len(attachments) == 0 {
		return nil
	}
	urls := make(map[string]string, len(attachments))
	for _, attachment := range attachments {
		if attachment.ObjectID != "" && attachment.URL != "" {
			urls[attachment.ObjectID] = attachment.URL
		}
	}
	return urls
}

// archiveTransportRef builds the same MediaRef the live path builds, then swaps in the archive URL
// when the transport's ancillary names an attachment this page carried.
func archiveTransportRef(mediaType whatsmeow.MediaType, transport *waMediaTransport.WAMediaTransport, width, height uint32, urls map[string]string) *MediaRef {
	ref := transportRef(mediaType, transport, width, height)
	if ref == nil || transport == nil {
		return ref
	}
	if url := urls[transport.GetAncillary().GetObjectID()]; url != "" {
		ref.RemoteRef = url
	}
	return ref
}

func archiveImageRef(message *waConsumerApplication.ConsumerApplication_ImageMessage, urls map[string]string) *MediaRef {
	transport, err := message.Decode()
	if err != nil {
		return nil
	}
	return archiveTransportRef(whatsmeow.MediaImage, transport.Integral.GetTransport(),
		transport.Ancillary.GetWidth(), transport.Ancillary.GetHeight(), urls)
}

func archiveVideoRef(message *waConsumerApplication.ConsumerApplication_VideoMessage, urls map[string]string) *MediaRef {
	transport, err := message.Decode()
	if err != nil {
		return nil
	}
	return archiveTransportRef(whatsmeow.MediaVideo, transport.Integral.GetTransport(),
		transport.Ancillary.GetWidth(), transport.Ancillary.GetHeight(), urls)
}

func archiveAudioRef(message *waConsumerApplication.ConsumerApplication_AudioMessage, urls map[string]string) *MediaRef {
	transport, err := message.Decode()
	if err != nil {
		return nil
	}
	return archiveTransportRef(whatsmeow.MediaAudio, transport.Integral.GetTransport(), 0, 0, urls)
}

func archiveDocumentRef(message *waConsumerApplication.ConsumerApplication_DocumentMessage, urls map[string]string) *MediaRef {
	transport, err := message.Decode()
	if err != nil {
		return nil
	}
	return archiveTransportRef(whatsmeow.MediaDocument, transport.Integral.GetTransport(), 0, 0, urls)
}

func archiveStickerRef(message *waConsumerApplication.ConsumerApplication_StickerMessage, urls map[string]string) *MediaRef {
	transport, err := message.Decode()
	if err != nil {
		return nil
	}
	ref := archiveTransportRef(whatsmeow.MediaImage, transport.Integral.GetTransport(),
		transport.Ancillary.GetWidth(), transport.Ancillary.GetHeight(), urls)
	if ref != nil {
		ref.Mime = "image/webp"
	}
	return ref
}

// errUnsupportedArchivePayload marks archive stanzas that carry no message this connector maps
// (events, admin messages and locally transformed copies).
var errUnsupportedArchivePayload = errors.New("unsupported Messenger archive payload")
