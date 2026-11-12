package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"go.mau.fi/mautrix-meta/pkg/messagix"
	"go.mau.fi/mautrix-meta/pkg/messagix/httpclient"
	"go.mau.fi/mautrix-meta/pkg/messagix/methods"
	"go.mau.fi/mautrix-meta/pkg/messagix/socket"
	"go.mau.fi/mautrix-meta/pkg/messagix/table"
)

func (m *Messenger) requireClient() (*messagix.Client, *WireError) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.cli == nil || m.loggedOut {
		return nil, errUnauthorized()
	}
	return m.cli, nil
}

func (m *Messenger) sendMessage(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p SendParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("send params: " + err.Error())
	}
	msg := p.Message
	if strings.TrimSpace(msg.TxnID) == "" {
		return nil, errInvalidInput("txn_id must not be empty")
	}
	body := ""
	if msg.Body != nil {
		body = *msg.Body
	}
	if body == "" && msg.Attachment == nil {
		return nil, errInvalidInput("message needs a body or an attachment")
	}
	key, jid, isE2EE, err := parseChatID(msg.RemoteChatID)
	if err != nil {
		return nil, errInvalidInput(err.Error())
	}
	cli, werr := m.requireClient()
	if werr != nil {
		return nil, werr
	}
	if isE2EE {
		return m.sendE2EE(ctx, cli, msg, jid)
	}
	return m.sendMeta(ctx, cli, msg, key, body)
}

func (m *Messenger) sendMeta(ctx context.Context, cli *messagix.Client, msg OutgoingMessage, threadKey int64, body string) (any, *WireError) {
	m.mu.Lock()
	thread := m.threads[threadKey]
	var replySender int64
	if msg.ReplyTo != nil {
		if meta := m.messages[*msg.ReplyTo]; meta != nil {
			replySender = meta.SenderID
		}
	}
	chatID := msg.RemoteChatID
	if thread != nil {
		chatID = remoteChatID(threadKey, thread.Type)
	}
	m.mu.Unlock()
	if thread == nil {
		return nil, errInvalidInput(fmt.Sprintf("unknown chat %q", msg.RemoteChatID))
	}

	task := &socket.SendMessageTask{
		ThreadId:         threadKey,
		Otid:             methods.GenerateEpochID(),
		Source:           table.MESSENGER_INBOX_IN_THREAD,
		InitiatingSource: table.FACEBOOK_INBOX,
		SendType:         table.TEXT,
		SyncGroup:        1,
		Text:             body,
	}
	if thread.Type == table.COMMUNITY_GROUP {
		task.SyncGroup = 104
	}
	if strings.Contains(body, "http://") || strings.Contains(body, "https://") {
		task.TextHasLinks = 1
	}
	if msg.ReplyTo != nil {
		task.ReplyMetaData = &socket.ReplyMetaData{
			ReplyMessageId:  *msg.ReplyTo,
			ReplySourceType: 1,
			ReplyType:       0,
			ReplySender:     replySender,
		}
	}
	if msg.Attachment != nil {
		attachmentID, werr := m.uploadAttachment(ctx, cli, threadKey, msg.Attachment)
		if werr != nil {
			return nil, werr
		}
		task.SendType = table.MEDIA
		task.AttachmentFBIds = []int64{attachmentID}
	} else {
		task.AttachmentFBIds = nil
	}

	otid := strconv.FormatInt(task.Otid, 10)
	m.mu.Lock()
	m.txns[otid] = msg.TxnID
	m.mu.Unlock()

	if err := cli.WaitUntilCanSendMessages(ctx, 15*time.Second); err != nil {
		m.log.Warn().Err(err).Msg("Socket not ready to send; trying anyway")
	}
	resp, err := cli.ExecuteTasks(ctx, task)
	if err != nil {
		return nil, mapMetaError(err)
	}
	if resp == nil {
		return nil, errProtocol("send response was empty")
	}
	remoteID := ""
	for _, replace := range resp.LSReplaceOptimsiticMessage {
		if replace.OfflineThreadingId == otid {
			remoteID = replace.MessageId
		}
	}
	if remoteID == "" {
		for _, inserted := range resp.LSInsertMessage {
			if inserted.OfflineThreadingId == otid {
				remoteID = inserted.MessageId
			}
		}
	}
	if remoteID == "" {
		for _, failed := range resp.LSMarkOptimisticMessageFailed {
			if failed.OTID == "" || failed.OTID == otid {
				return nil, errProtocol("server rejected the message: " + failed.Message)
			}
		}
		for _, failed := range resp.LSHandleFailedTask {
			if failed.OTID == "" || failed.OTID == otid {
				return nil, errProtocol("server rejected the message: " + failed.Message)
			}
		}
		return nil, errProtocol("send response did not include the message id")
	}

	sentAt := time.Now().UnixMilli()
	if parsed, err := methods.ParseMessageID(remoteID); err == nil && parsed > 0 {
		sentAt = parsed
	}
	m.mu.Lock()
	m.rememberMessageLocked(remoteID, &msgMeta{
		ThreadKey:   threadKey,
		ChatID:      chatID,
		SenderID:    m.ownID,
		IsOwn:       true,
		TimestampMs: sentAt,
		Text:        body,
	})
	m.mu.Unlock()
	return SendAck{TxnID: msg.TxnID, RemoteMsgID: remoteID, SentAt: sentAt}, nil
}

// uploadAttachment sends one file through the Messenger media upload endpoint and returns the
// attachment FBID used by SendMessageTask.
func (m *Messenger) uploadAttachment(ctx context.Context, cli *messagix.Client, threadKey int64, att *LocalAttachment) (int64, *WireError) {
	info, err := os.Stat(att.Path)
	if err != nil {
		return 0, errInvalidInput(fmt.Sprintf("attachment %q: %v", att.Path, err))
	}
	if !info.Mode().IsRegular() {
		return 0, errInvalidInput(fmt.Sprintf("attachment %q is not a regular file", att.Path))
	}
	if info.Size() > maxUploadBytes {
		return 0, errInvalidInput(fmt.Sprintf("attachment is %d bytes; the network accepts at most %d", info.Size(), maxUploadBytes))
	}
	data, err := os.ReadFile(att.Path)
	if err != nil {
		return 0, errInvalidInput(fmt.Sprintf("attachment %q: %v", att.Path, err))
	}
	mime := ""
	if att.Mime != nil {
		mime = *att.Mime
	}
	if mime == "" {
		mime = http.DetectContentType(data)
	}
	name := filepath.Base(att.Path)
	if att.FileName != nil && *att.FileName != "" {
		name = *att.FileName
	}
	resp, err := cli.GetHTTP().SendMercuryUploadRequest(ctx, threadKey, &httpclient.MercuryUploadMedia{
		Filename:  name,
		MimeType:  mime,
		MediaData: data,
	})
	if err != nil {
		return 0, mapMetaError(err)
	}
	if resp.Payload.RealMetadata == nil {
		return 0, errProtocol("upload response had no metadata")
	}
	fbid := resp.Payload.RealMetadata.GetFbId()
	if fbid == 0 {
		return 0, errProtocol("upload response had no attachment id")
	}
	return fbid, nil
}

// editWindowMillis is Capabilities.EditWindowSecs in milliseconds.
const editWindowMillis = 900 * 1000

func (m *Messenger) editMessage(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p EditParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("edit params: " + err.Error())
	}
	if p.Body == "" {
		return nil, errInvalidInput("edit body must not be empty")
	}
	if p.Formatted != nil && *p.Formatted != "" {
		return nil, errNotSupported("formatted_text")
	}
	m.mu.Lock()
	original := m.messages[p.RemoteMsgID]
	m.mu.Unlock()
	if original != nil && original.TimestampMs > 0 && time.Now().UnixMilli()-original.TimestampMs > editWindowMillis {
		return nil, errNotSupported("edit_window")
	}
	_, jid, isE2EE, err := parseChatID(p.RemoteChatID)
	if err != nil {
		return nil, errInvalidInput(err.Error())
	}
	cli, werr := m.requireClient()
	if werr != nil {
		return nil, werr
	}
	if isE2EE {
		return m.editE2EE(ctx, cli, jid, p)
	}
	resp, err := cli.ExecuteTasks(ctx, &socket.EditMessageTask{MessageID: p.RemoteMsgID, Text: p.Body})
	if err != nil {
		return nil, mapMetaError(err)
	}
	if resp == nil || len(resp.LSEditMessage) == 0 {
		return nil, errProtocol("edit was not acknowledged by the network")
	}
	if resp.LSEditMessage[0].Text != p.Body {
		return nil, errProtocol("network rejected the edit")
	}
	m.mu.Lock()
	if meta := m.messages[p.RemoteMsgID]; meta != nil {
		meta.Text = p.Body
		meta.EditCount = resp.LSEditMessage[0].EditCount
	}
	m.mu.Unlock()
	return nil, nil
}

func (m *Messenger) deleteMessage(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p DeleteParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("delete params: " + err.Error())
	}
	key, jid, isE2EE, err := parseChatID(p.RemoteChatID)
	if err != nil {
		return nil, errInvalidInput(err.Error())
	}
	cli, werr := m.requireClient()
	if werr != nil {
		return nil, werr
	}
	if isE2EE {
		return m.deleteE2EE(ctx, cli, jid, p)
	}
	var task socket.Task = &socket.DeleteMessageTask{MessageId: p.RemoteMsgID}
	if !p.ForEveryone {
		task = &socket.DeleteMessageMeOnlyTask{ThreadKey: key, MessageId: p.RemoteMsgID}
	}
	if _, err := cli.ExecuteTasks(ctx, task); err != nil {
		return nil, mapMetaError(err)
	}
	m.mu.Lock()
	delete(m.messages, p.RemoteMsgID)
	m.mu.Unlock()
	return nil, nil
}

func (m *Messenger) react(ctx context.Context, params json.RawMessage, add bool) (any, *WireError) {
	var p ReactParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("react params: " + err.Error())
	}
	if add && p.Emoji == "" {
		return nil, errInvalidInput("emoji must not be empty")
	}
	key, jid, isE2EE, err := parseChatID(p.RemoteChatID)
	if err != nil {
		return nil, errInvalidInput(err.Error())
	}
	cli, werr := m.requireClient()
	if werr != nil {
		return nil, werr
	}
	if isE2EE {
		return m.reactE2EE(ctx, cli, jid, p, add)
	}
	m.mu.Lock()
	thread := m.threads[key]
	actor := m.ownID
	m.mu.Unlock()
	if thread == nil {
		return nil, errInvalidInput(fmt.Sprintf("unknown chat %q", p.RemoteChatID))
	}
	emoji := ""
	if add {
		emoji = p.Emoji
	}
	if _, err := cli.ExecuteTasks(ctx, &socket.SendReactionTask{
		ThreadKey:       key,
		MessageID:       p.RemoteMsgID,
		ActorID:         actor,
		Reaction:        emoji,
		SyncGroup:       1,
		SendAttribution: table.MESSENGER_INBOX_IN_THREAD,
	}); err != nil {
		return nil, mapMetaError(err)
	}
	return nil, nil
}

func (m *Messenger) markRead(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p MarkReadParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("mark_read params: " + err.Error())
	}
	key, jid, isE2EE, err := parseChatID(p.RemoteChatID)
	if err != nil {
		return nil, errInvalidInput(err.Error())
	}
	cli, werr := m.requireClient()
	if werr != nil {
		return nil, werr
	}
	if isE2EE {
		return m.markReadE2EE(ctx, cli, jid, p)
	}
	ts, err := methods.ParseMessageID(p.UpToRemoteMsgID)
	if err != nil || ts <= 0 {
		return nil, errInvalidInput(fmt.Sprintf("unknown message id %q", p.UpToRemoteMsgID))
	}
	if _, err := cli.ExecuteTasks(ctx, &socket.ThreadMarkReadTask{
		ThreadId:            key,
		LastReadWatermarkTs: ts,
		SyncGroup:           1,
	}); err != nil {
		return nil, mapMetaError(err)
	}
	return nil, nil
}

func (m *Messenger) setTyping(ctx context.Context, params json.RawMessage) (any, *WireError) {
	var p TypingParams
	if err := json.Unmarshal(params, &p); err != nil {
		return nil, errInvalidInput("set_typing params: " + err.Error())
	}
	key, jid, isE2EE, err := parseChatID(p.RemoteChatID)
	if err != nil {
		return nil, errInvalidInput(err.Error())
	}
	cli, werr := m.requireClient()
	if werr != nil {
		return nil, werr
	}
	if isE2EE {
		return m.setTypingE2EE(ctx, cli, jid, p.Typing)
	}
	threadType, ok := m.threadTypeKnown(key)
	if !ok {
		return nil, errInvalidInput(fmt.Sprintf("unknown chat %q", p.RemoteChatID))
	}
	isGroup := int64(1)
	if threadType.IsOneToOne() {
		isGroup = 0
	}
	isTyping := int64(0)
	if p.Typing {
		isTyping = 1
	}
	if err := cli.ExecuteStatelessTask(ctx, &socket.UpdatePresenceTask{
		ThreadKey:     key,
		IsGroupThread: isGroup,
		IsTyping:      isTyping,
		Attribution:   0,
		SyncGroup:     1,
		ThreadType:    int64(threadType),
	}); err != nil {
		return nil, mapMetaError(err)
	}
	return nil, nil
}
