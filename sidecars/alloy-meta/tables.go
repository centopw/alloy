package main

import (
	"strconv"
	"time"

	"go.mau.fi/mautrix-meta/pkg/messagix/methods"
	"go.mau.fi/mautrix-meta/pkg/messagix/table"
)

// maxIndexedMessages bounds the in-memory message index. Editing and reactions need the
// mapping, but only for recent history; older entries are dropped and callers fall back to
// treating the message as one of ours when they must build a WhatsApp key.
const maxIndexedMessages = 5000

// attachmentsByMessage folds attachment rows of one table into MediaRefs keyed by message id.
func attachmentsByMessage(tbl *table.LSTable) map[string]*MediaRef {
	out := make(map[string]*MediaRef, len(tbl.LSInsertAttachment)+len(tbl.LSInsertBlobAttachment))
	for _, att := range tbl.LSInsertAttachment {
		if ref := attachmentRef(att.Filename, att.Filesize, att.AttachmentMimeType, att.PlayableUrl,
			att.PlayableDurationMs, att.PreviewWidth, att.PreviewHeight); ref != nil {
			out[att.MessageId] = ref
		}
	}
	for _, att := range tbl.LSInsertBlobAttachment {
		if ref := attachmentRef(att.Filename, att.Filesize, att.AttachmentMimeType, att.PlayableUrl,
			att.PlayableDurationMs, att.PreviewWidth, att.PreviewHeight); ref != nil {
			out[att.MessageId] = ref
		}
	}
	for _, att := range tbl.LSInsertStickerAttachment {
		ref := attachmentRef("", 0, att.PlayableUrlMimeType, att.PlayableUrl, 0, att.PreviewWidth, att.PreviewHeight)
		if ref != nil {
			ref.Mime = "image/webp"
			out[att.MessageId] = ref
		}
	}
	return out
}

func attachmentRef(name string, size int64, mime, url string, durationMs, width, height int64) *MediaRef {
	if url == "" {
		return nil
	}
	ref := &MediaRef{RemoteRef: url, Mime: mime}
	if ref.Mime == "" {
		ref.Mime = "application/octet-stream"
	}
	if size > 0 {
		ref.SizeBytes = new(uint64(size))
	}
	if width > 0 {
		ref.Width = new(uint32(width))
	}
	if height > 0 {
		ref.Height = new(uint32(height))
	}
	if durationMs > 0 {
		ref.DurationMs = new(uint64(durationMs))
	}
	if name != "" {
		ref.FileName = &name
	}
	return ref
}

// insertFromDeleteThenInsert adapts an unsend/rewrite row to the common message shape.
func insertFromDeleteThenInsert(row *table.LSDeleteThenInsertMessage) *table.LSInsertMessage {
	return &table.LSInsertMessage{
		Text:               row.Text,
		ThreadKey:          row.ThreadKey,
		TimestampMs:        row.TimestampMs,
		MessageId:          row.MessageId,
		OfflineThreadingId: row.OfflineThreadingId,
		SenderId:           row.SenderId,
		StickerId:          row.StickerId,
		IsAdminMessage:     row.IsAdminMessage,
		IsUnsent:           row.IsUnsent,
		ReplySourceId:      row.ReplySourceId,
		EditCount:          row.EditCount,
	}
}

func kindForAttachment(ref *MediaRef, msg *table.LSInsertMessage) string {
	if msg.IsAdminMessage {
		return "system"
	}
	if ref == nil {
		if msg.StickerId != 0 {
			return "sticker"
		}
		return "text"
	}
	switch {
	case ref.Mime == "image/webp" && msg.StickerId != 0:
		return "sticker"
	case len(ref.Mime) >= 6 && ref.Mime[:6] == "image/":
		return "image"
	case len(ref.Mime) >= 6 && ref.Mime[:6] == "video/":
		return "video"
	case len(ref.Mime) >= 6 && ref.Mime[:6] == "audio/":
		return "audio"
	default:
		return "file"
	}
}

// messageFromRow converts a table message row into the connector shape.
func (m *Messenger) messageFromRowLocked(row *table.LSInsertMessage, refs map[string]*MediaRef) (RemoteMessage, *msgMeta) {
	thread := m.threads[row.ThreadKey]
	chatID := strconv.FormatInt(row.ThreadKey, 10)
	if thread != nil {
		chatID = thread.chatID()
	}
	sentAt := row.TimestampMs
	if sentAt == 0 {
		if parsed, err := methods.ParseMessageID(row.MessageId); err == nil {
			sentAt = parsed
		}
	}
	var txn *string
	if row.OfflineThreadingId != "" {
		if txnID, ok := m.txns[row.OfflineThreadingId]; ok {
			txn = &txnID
		}
	}
	var body *string
	if row.Text != "" {
		body = &row.Text
	}
	var reply *string
	if row.ReplySourceId != "" {
		reply = &row.ReplySourceId
	}
	ref := refs[row.MessageId]
	msg := RemoteMessage{
		RemoteChatID:       chatID,
		RemoteMsgID:        row.MessageId,
		TxnID:              txn,
		SenderRemoteID:     strconv.FormatInt(row.SenderId, 10),
		IsOwn:              row.SenderId != 0 && row.SenderId == m.ownID,
		SentAt:             sentAt,
		EditedAt:           nil,
		Kind:               kindForAttachment(ref, row),
		Body:               body,
		Formatted:          nil,
		ReplyToRemoteMsgID: reply,
		Media:              ref,
	}
	meta := &msgMeta{
		ThreadKey:   row.ThreadKey,
		ChatID:      chatID,
		SenderID:    row.SenderId,
		IsOwn:       msg.IsOwn,
		TimestampMs: sentAt,
		Text:        row.Text,
		EditCount:   row.EditCount,
	}
	return msg, meta
}

// rememberMessageLocked records a message so edits, reactions and receipts can resolve it.
func (m *Messenger) rememberMessageLocked(id string, meta *msgMeta) {
	if id == "" || meta == nil {
		return
	}
	m.messages[id] = meta
	if len(m.messages) > maxIndexedMessages {
		var oldestID string
		var oldest int64 = 1<<63 - 1
		for key, candidate := range m.messages {
			if candidate.TimestampMs < oldest {
				oldest = candidate.TimestampMs
				oldestID = key
			}
		}
		delete(m.messages, oldestID)
	}
}

// newestMessageAtOrBeforeLocked finds the newest indexed message id in a thread at or before a
// timestamp, which is how Messenger read receipts identify their target.
func (m *Messenger) newestMessageAtOrBeforeLocked(threadKey int64, ts int64) string {
	var bestID string
	var bestTS int64 = -1
	for id, meta := range m.messages {
		if meta.ThreadKey != threadKey || meta.TimestampMs > ts || meta.TimestampMs < bestTS {
			continue
		}
		bestTS = meta.TimestampMs
		bestID = id
	}
	return bestID
}

// messageEventsLocked turns message-bearing rows into events. mu must be held.
func (m *Messenger) messageEventsLocked(tbl *table.LSTable) []any {
	var events []any
	refs := attachmentsByMessage(tbl)

	insert := func(row *table.LSInsertMessage) {
		thread := m.threads[row.ThreadKey]
		if thread == nil {
			m.log.Debug().Int64("thread_key", row.ThreadKey).Str("message_id", row.MessageId).
				Msg("Dropping message for unknown thread")
			return
		}
		if thread.Type.IsWhatsApp() {
			// Encrypted content arrives over the E2EE client; the table only has a stub.
			return
		}
		if previous := m.messages[row.MessageId]; previous != nil {
			if row.Text != "" && previous.Text != row.Text {
				body := row.Text
				editedAt := row.TimestampMs
				if editedAt == 0 {
					editedAt = time.Now().UnixMilli()
				}
				events = append(events, NewMessageEditedEvent(thread.chatID(), row.MessageId, &body, editedAt))
				previous.Text = row.Text
				previous.EditCount = row.EditCount
			}
			return
		}
		msg, meta := m.messageFromRowLocked(row, refs)
		m.rememberMessageLocked(row.MessageId, meta)
		events = append(events, NewMessageEvent(msg))
	}

	for _, row := range tbl.LSInsertMessage {
		if row.IsUnsent {
			events = append(events, m.deletedLocked(row.ThreadKey, row.MessageId))
			continue
		}
		insert(row)
	}
	for _, row := range tbl.LSUpsertMessage {
		if row.IsUnsent {
			events = append(events, m.deletedLocked(row.ThreadKey, row.MessageId))
			continue
		}
		insert(row.ToInsert())
	}
	for _, row := range tbl.LSDeleteThenInsertMessage {
		if row.IsUnsent {
			events = append(events, m.deletedLocked(row.ThreadKey, row.MessageId))
			continue
		}
		insert(insertFromDeleteThenInsert(row))
	}
	for _, row := range tbl.LSDeleteMessage {
		events = append(events, m.deletedLocked(row.ThreadKey, row.MessageId))
	}
	for _, row := range tbl.LSEditMessage {
		meta := m.messages[row.MessageID]
		if meta == nil {
			continue
		}
		thread := m.threads[meta.ThreadKey]
		if thread == nil {
			continue
		}
		body := row.Text
		events = append(events, NewMessageEditedEvent(thread.chatID(), row.MessageID, &body, time.Now().UnixMilli()))
		meta.Text = row.Text
		meta.EditCount = row.EditCount
	}
	for _, row := range tbl.LSUpsertReaction {
		thread := m.threads[row.ThreadKey]
		if thread == nil || row.Reaction == "" {
			continue
		}
		events = append(events, NewReactionEvent(thread.chatID(), row.MessageId,
			strconv.FormatInt(row.ActorId, 10), row.Reaction, true))
	}
	for _, row := range tbl.LSDeleteReaction {
		thread := m.threads[row.ThreadKey]
		if thread == nil {
			continue
		}
		events = append(events, NewReactionEvent(thread.chatID(), row.MessageId,
			strconv.FormatInt(row.ActorId, 10), "", false))
	}
	for _, row := range tbl.LSUpdateReadReceipt {
		thread := m.threads[row.ThreadKey]
		if thread == nil {
			continue
		}
		if msgID := m.newestMessageAtOrBeforeLocked(row.ThreadKey, row.ReadWatermarkTimestampMs); msgID != "" {
			events = append(events, NewReadMarkerEvent(thread.chatID(),
				strconv.FormatInt(row.ContactId, 10), msgID))
		}
	}
	for _, row := range tbl.LSUpdateTypingIndicator {
		thread := m.threads[row.ThreadKey]
		if thread == nil {
			continue
		}
		events = append(events, NewTypingEvent(thread.chatID(),
			strconv.FormatInt(row.SenderId, 10), row.IsTyping))
	}
	for _, row := range tbl.LSReplaceOptimsiticMessage {
		if txnID, ok := m.txns[row.OfflineThreadingId]; ok {
			if meta := m.messages[row.MessageId]; meta == nil {
				m.log.Debug().Str("otid", row.OfflineThreadingId).Str("message_id", row.MessageId).
					Str("txn_id", txnID).Msg("Send echo arrived before the table row")
			}
		}
	}
	return events
}

func (m *Messenger) deletedLocked(threadKey int64, msgID string) any {
	thread := m.threads[threadKey]
	if thread == nil {
		if meta := m.messages[msgID]; meta != nil {
			return NewMessageDeletedEvent(meta.ChatID, msgID)
		}
		return nil
	}
	delete(m.messages, msgID)
	return NewMessageDeletedEvent(thread.chatID(), msgID)
}
