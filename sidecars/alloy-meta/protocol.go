// Package main implements Alloy's Messenger connector sidecar.
//
// The sidecar speaks the NDJSON connector protocol defined by
// crates/alloy-connector/src/rpc.rs (documented in docs/plan/10-connector-protocol.md).
// Every struct in this file mirrors a Rust serde shape one-for-one: field names are
// snake_case, unit enums are bare strings, data-carrying enums are internally tagged with
// "type", timestamps are milliseconds since the Unix epoch, and Option fields are emitted as
// null rather than omitted.
package main

import (
	"encoding/json"
	"fmt"
)

const protocolVersion = 1

const networkMessenger = "messenger"

// Capabilities mirrors alloy_connector::Capabilities.
type Capabilities struct {
	Edit                bool    `json:"edit"`
	EditWindowSecs      *uint32 `json:"edit_window_secs"`
	DeleteForEveryone   bool    `json:"delete_for_everyone"`
	Reactions           bool    `json:"reactions"`
	MaxReactionsPerUser *uint8  `json:"max_reactions_per_user"`
	Replies             bool    `json:"replies"`
	Threads             bool    `json:"threads"`
	Typing              bool    `json:"typing"`
	ReadReceipts        bool    `json:"read_receipts"`
	DeliveredReceipts   bool    `json:"delivered_receipts"`
	Voice               bool    `json:"voice"`
	Polls               bool    `json:"polls"`
	Location            bool    `json:"location"`
	MaxUploadBytes      *uint64 `json:"max_upload_bytes"`
	FormattedText       bool    `json:"formatted_text"`
}

// MessengerCapabilities is the fixed capability set this sidecar promises. Changing a value
// here is a contract change: the host hides actions based on it.
func MessengerCapabilities() Capabilities {
	return Capabilities{
		Edit:                true,
		EditWindowSecs:      new(uint32(900)),
		DeleteForEveryone:   true,
		Reactions:           true,
		MaxReactionsPerUser: new(uint8(1)),
		Replies:             true,
		Threads:             false,
		Typing:              true,
		ReadReceipts:        true,
		DeliveredReceipts:   true,
		Voice:               true,
		Polls:               false,
		Location:            false,
		MaxUploadBytes:      new(uint64(26214400)),
		FormattedText:       false,
	}
}

// Hello is the first frame the sidecar writes.
type Hello struct {
	Type            string       `json:"type"`
	ProtocolVersion uint32       `json:"protocol_version"`
	Network         string       `json:"network"`
	Capabilities    Capabilities `json:"capabilities"`
}

func NewHello() Hello {
	return Hello{
		Type:            "hello",
		ProtocolVersion: protocolVersion,
		Network:         networkMessenger,
		Capabilities:    MessengerCapabilities(),
	}
}

// Frame types the host sends.
type Request struct {
	Type   string          `json:"type"`
	ID     uint64          `json:"id"`
	Method string          `json:"method"`
	Params json.RawMessage `json:"params"`
}

// Response answers exactly one Request. Exactly one of Result/Error is non-nil; both keys are
// always present so the Rust Option<Value>/Option<ConnectorError> shapes decode.
type Response struct {
	Type   string     `json:"type"`
	ID     uint64     `json:"id"`
	Result any        `json:"result"`
	Error  *WireError `json:"error"`
}

// EventFrame carries one ConnectorEvent.
type EventFrame struct {
	Type  string `json:"type"`
	Event any    `json:"event"`
}

// WireError is ConnectorError: internally tagged with "type", one optional payload field.
type WireError struct {
	Type         string  `json:"type"`
	RetryAfterMs *uint64 `json:"retry_after_ms,omitempty"`
	Feature      string  `json:"feature,omitempty"`
	Message      string  `json:"message,omitempty"`
}

func (e *WireError) Error() string {
	switch e.Type {
	case "rate_limited":
		if e.RetryAfterMs != nil {
			return fmt.Sprintf("rate limited: retry after %d ms", *e.RetryAfterMs)
		}
		return "rate limited"
	case "not_supported":
		return "unsupported feature: " + e.Feature
	case "unauthorized":
		return "unauthorized: re-login required"
	default:
		return e.Type + ": " + e.Message
	}
}

func errUnauthorized() *WireError { return &WireError{Type: "unauthorized"} }

func errRateLimited(retryAfterMs uint64) *WireError {
	return &WireError{Type: "rate_limited", RetryAfterMs: &retryAfterMs}
}

func errNotSupported(feature string) *WireError {
	return &WireError{Type: "not_supported", Feature: feature}
}

func errNetwork(message string) *WireError {
	return &WireError{Type: "network", Message: message}
}

func errProtocol(message string) *WireError {
	return &WireError{Type: "protocol", Message: message}
}

func errInvalidInput(message string) *WireError {
	return &WireError{Type: "invalid_input", Message: message}
}

// ConnectionState mirrors alloy_connector::ConnectionState.
type ConnectionState struct {
	Type   string  `json:"type"`
	Reason *string `json:"reason,omitempty"`
}

func stateConnecting() ConnectionState { return ConnectionState{Type: "connecting"} }
func stateOnline() ConnectionState     { return ConnectionState{Type: "online"} }
func stateNeedsRelogin() ConnectionState {
	return ConnectionState{Type: "needs_relogin"}
}

func stateOffline(reason string) ConnectionState {
	return ConnectionState{Type: "offline", Reason: &reason}
}

// ConnectionEvent is ConnectorEvent::Connection.
type ConnectionEvent struct {
	Type  string          `json:"type"`
	State ConnectionState `json:"state"`
}

func NewConnectionEvent(state ConnectionState) ConnectionEvent {
	return ConnectionEvent{Type: "connection", State: state}
}

// MediaRef mirrors alloy_connector::MediaRef.
type MediaRef struct {
	RemoteRef  string  `json:"remote_ref"`
	Mime       string  `json:"mime"`
	SizeBytes  *uint64 `json:"size_bytes"`
	Width      *uint32 `json:"width"`
	Height     *uint32 `json:"height"`
	DurationMs *uint64 `json:"duration_ms"`
	FileName   *string `json:"file_name"`
}

// RemoteChat mirrors alloy_connector::RemoteChat.
type RemoteChat struct {
	RemoteChatID   string    `json:"remote_chat_id"`
	Kind           string    `json:"kind"`
	Title          string    `json:"title"`
	Avatar         *MediaRef `json:"avatar"`
	LastActivityAt int64     `json:"last_activity_at"`
	UnreadCount    uint32    `json:"unread_count"`
	IsPinned       bool      `json:"is_pinned"`
	IsArchived     bool      `json:"is_archived"`
	IsMuted        bool      `json:"is_muted"`
}

// RemoteUser mirrors alloy_connector::RemoteUser.
type RemoteUser struct {
	RemoteUserID string    `json:"remote_user_id"`
	DisplayName  string    `json:"display_name"`
	Avatar       *MediaRef `json:"avatar"`
}

// RemoteMessage mirrors alloy_connector::RemoteMessage.
type RemoteMessage struct {
	RemoteChatID       string    `json:"remote_chat_id"`
	RemoteMsgID        string    `json:"remote_msg_id"`
	TxnID              *string   `json:"txn_id"`
	SenderRemoteID     string    `json:"sender_remote_id"`
	IsOwn              bool      `json:"is_own"`
	SentAt             int64     `json:"sent_at"`
	EditedAt           *int64    `json:"edited_at"`
	Kind               string    `json:"kind"`
	Body               *string   `json:"body"`
	Formatted          *string   `json:"formatted"`
	ReplyToRemoteMsgID *string   `json:"reply_to_remote_msg_id"`
	Media              *MediaRef `json:"media"`
}

// ChatUpsertedEvent is ConnectorEvent::ChatUpserted.
type ChatUpsertedEvent struct {
	Type string     `json:"type"`
	Chat RemoteChat `json:"chat"`
}

func NewChatUpsertedEvent(chat RemoteChat) ChatUpsertedEvent {
	return ChatUpsertedEvent{Type: "chat_upserted", Chat: chat}
}

// ChatRemovedEvent is ConnectorEvent::ChatRemoved.
type ChatRemovedEvent struct {
	Type         string `json:"type"`
	RemoteChatID string `json:"remote_chat_id"`
}

func NewChatRemovedEvent(chatID string) ChatRemovedEvent {
	return ChatRemovedEvent{Type: "chat_removed", RemoteChatID: chatID}
}

// MessageEvent is ConnectorEvent::Message.
type MessageEvent struct {
	Type    string        `json:"type"`
	Message RemoteMessage `json:"message"`
}

func NewMessageEvent(message RemoteMessage) MessageEvent {
	return MessageEvent{Type: "message", Message: message}
}

// MessageEditedEvent is ConnectorEvent::MessageEdited.
type MessageEditedEvent struct {
	Type         string  `json:"type"`
	RemoteChatID string  `json:"remote_chat_id"`
	RemoteMsgID  string  `json:"remote_msg_id"`
	Body         *string `json:"body"`
	Formatted    *string `json:"formatted"`
	EditedAt     int64   `json:"edited_at"`
}

func NewMessageEditedEvent(chatID, msgID string, body *string, editedAt int64) MessageEditedEvent {
	return MessageEditedEvent{
		Type:         "message_edited",
		RemoteChatID: chatID,
		RemoteMsgID:  msgID,
		Body:         body,
		Formatted:    nil,
		EditedAt:     editedAt,
	}
}

// MessageDeletedEvent is ConnectorEvent::MessageDeleted.
type MessageDeletedEvent struct {
	Type         string `json:"type"`
	RemoteChatID string `json:"remote_chat_id"`
	RemoteMsgID  string `json:"remote_msg_id"`
}

func NewMessageDeletedEvent(chatID, msgID string) MessageDeletedEvent {
	return MessageDeletedEvent{Type: "message_deleted", RemoteChatID: chatID, RemoteMsgID: msgID}
}

// ReactionEvent is ConnectorEvent::Reaction.
type ReactionEvent struct {
	Type           string `json:"type"`
	RemoteChatID   string `json:"remote_chat_id"`
	RemoteMsgID    string `json:"remote_msg_id"`
	SenderRemoteID string `json:"sender_remote_id"`
	Emoji          string `json:"emoji"`
	Added          bool   `json:"added"`
}

func NewReactionEvent(chatID, msgID, senderID, emoji string, added bool) ReactionEvent {
	return ReactionEvent{
		Type:           "reaction",
		RemoteChatID:   chatID,
		RemoteMsgID:    msgID,
		SenderRemoteID: senderID,
		Emoji:          emoji,
		Added:          added,
	}
}

// ReadMarkerEvent is ConnectorEvent::ReadMarker.
type ReadMarkerEvent struct {
	Type            string `json:"type"`
	RemoteChatID    string `json:"remote_chat_id"`
	RemoteUserID    string `json:"remote_user_id"`
	UpToRemoteMsgID string `json:"up_to_remote_msg_id"`
}

func NewReadMarkerEvent(chatID, userID, msgID string) ReadMarkerEvent {
	return ReadMarkerEvent{
		Type:            "read_marker",
		RemoteChatID:    chatID,
		RemoteUserID:    userID,
		UpToRemoteMsgID: msgID,
	}
}

// TypingEvent is ConnectorEvent::Typing.
type TypingEvent struct {
	Type         string `json:"type"`
	RemoteChatID string `json:"remote_chat_id"`
	RemoteUserID string `json:"remote_user_id"`
	Typing       bool   `json:"typing"`
}

func NewTypingEvent(chatID, userID string, typing bool) TypingEvent {
	return TypingEvent{
		Type:         "typing",
		RemoteChatID: chatID,
		RemoteUserID: userID,
		Typing:       typing,
	}
}

// SessionUpdatedEvent is ConnectorEvent::SessionUpdated. Session is a base64 SessionBlob.
type SessionUpdatedEvent struct {
	Type    string `json:"type"`
	Session string `json:"session"`
}

func NewSessionUpdatedEvent(session string) SessionUpdatedEvent {
	return SessionUpdatedEvent{Type: "session_updated", Session: session}
}

// SendAck mirrors alloy_connector::SendAck.
type SendAck struct {
	TxnID       string `json:"txn_id"`
	RemoteMsgID string `json:"remote_msg_id"`
	SentAt      int64  `json:"sent_at"`
}

// ChatPage mirrors Page<RemoteChat>.
type ChatPage struct {
	Items      []RemoteChat `json:"items"`
	NextCursor *string      `json:"next_cursor"`
}

// MessagePage mirrors Page<RemoteMessage>.
type MessagePage struct {
	Items      []RemoteMessage `json:"items"`
	NextCursor *string         `json:"next_cursor"`
}

// CookieField mirrors alloy_connector::CookieField.
type CookieField struct {
	Name     string `json:"name"`
	Label    string `json:"label"`
	Required bool   `json:"required"`
}

// LoginFailure mirrors alloy_connector::LoginFailure.
type LoginFailure struct {
	Type         string  `json:"type"`
	Field        *string `json:"field,omitempty"`
	Message      string  `json:"message,omitempty"`
	RetryAfterMs *uint64 `json:"retry_after_ms,omitempty"`
}

func failureInvalidInput(field, message string) LoginFailure {
	return LoginFailure{Type: "invalid_input", Field: &field, Message: message}
}

func failureAccountLocked(message string) LoginFailure {
	return LoginFailure{Type: "account_locked", Message: message}
}

func failureNetwork(message string) LoginFailure {
	return LoginFailure{Type: "network", Message: message}
}

func failureRateLimited(retryAfterMs uint64) LoginFailure {
	return LoginFailure{Type: "rate_limited", RetryAfterMs: &retryAfterMs}
}

func failureNotSupported() LoginFailure { return LoginFailure{Type: "not_supported"} }

// LoginStep mirrors alloy_connector::LoginStep. Only the fields of the active variant are
// emitted; every variant carries its own tag.
//
// Password steps carry no hint and code steps carry no length: this network never tells us either
// value, and the contract's Option fields decode a missing key as None.
type LoginStep struct {
	Type          string        `json:"type"`
	Fields        []CookieField `json:"fields,omitempty"`
	HelpURL       *string       `json:"help_url,omitempty"`
	Via           string        `json:"via,omitempty"`
	UsernameLabel string        `json:"username_label,omitempty"`
	Alternatives  []string      `json:"alternatives,omitempty"`
	RemoteUserID  string        `json:"remote_user_id,omitempty"`
	DisplayName   string        `json:"display_name,omitempty"`
	Session       string        `json:"session,omitempty"`
	Reason        *LoginFailure `json:"reason,omitempty"`
}

func cookiesStep(fields []CookieField, helpURL string) LoginStep {
	return LoginStep{Type: "cookies", Fields: fields, HelpURL: &helpURL}
}

// credentialsStep asks for the Facebook account's email/phone and password. Cookie import stays one
// click away as the fallback for accounts the Lite flow refuses.
func credentialsStep() LoginStep {
	return LoginStep{
		Type:          "credentials",
		UsernameLabel: liteUsernameLabel,
		Alternatives:  []string{"cookies"},
	}
}

// codeStep asks for a one-time code. via is the CodeChannel wire value.
func codeStep(via string) LoginStep {
	return LoginStep{Type: "code", Via: via}
}

// passwordStep asks for the account password on its own (the flow re-prompts after a rejection).
func passwordStep() LoginStep {
	return LoginStep{Type: "password"}
}

func doneStep(remoteUserID, displayName, session string) LoginStep {
	return LoginStep{
		Type:         "done",
		RemoteUserID: remoteUserID,
		DisplayName:  displayName,
		Session:      session,
	}
}

func failedStep(reason LoginFailure) LoginStep {
	return LoginStep{Type: "failed", Reason: &reason}
}

// LoginInput mirrors alloy_connector::LoginInput.
type LoginInput struct {
	Type     string            `json:"type"`
	Phone    string            `json:"phone,omitempty"`
	Code     string            `json:"code,omitempty"`
	Password string            `json:"password,omitempty"`
	Username string            `json:"username,omitempty"`
	To       string            `json:"to,omitempty"`
	Values   map[string]string `json:"values,omitempty"`
}

// LocalAttachment mirrors alloy_connector::LocalAttachment.
type LocalAttachment struct {
	Path     string  `json:"path"`
	Mime     *string `json:"mime"`
	FileName *string `json:"file_name"`
}

// OutgoingMessage mirrors alloy_connector::OutgoingMessage.
type OutgoingMessage struct {
	TxnID        string           `json:"txn_id"`
	RemoteChatID string           `json:"remote_chat_id"`
	Body         *string          `json:"body"`
	Formatted    *string          `json:"formatted"`
	ReplyTo      *string          `json:"reply_to"`
	Attachment   *LocalAttachment `json:"attachment"`
}

// Request parameter objects, keyed exactly like the trait arguments.

type RestoreParams struct {
	Session string `json:"session"`
}

type SubmitLoginParams struct {
	Input LoginInput `json:"input"`
}

type ListChatsParams struct {
	Cursor *string `json:"cursor"`
	Limit  uint64  `json:"limit"`
}

type BackfillParams struct {
	RemoteChatID string  `json:"remote_chat_id"`
	Before       *string `json:"before"`
	Limit        uint64  `json:"limit"`
}

type SendParams struct {
	Message OutgoingMessage `json:"message"`
}

type EditParams struct {
	RemoteChatID string  `json:"remote_chat_id"`
	RemoteMsgID  string  `json:"remote_msg_id"`
	Body         string  `json:"body"`
	Formatted    *string `json:"formatted"`
}

type DeleteParams struct {
	RemoteChatID string `json:"remote_chat_id"`
	RemoteMsgID  string `json:"remote_msg_id"`
	ForEveryone  bool   `json:"for_everyone"`
}

type ReactParams struct {
	RemoteChatID string `json:"remote_chat_id"`
	RemoteMsgID  string `json:"remote_msg_id"`
	Emoji        string `json:"emoji"`
}

type MarkReadParams struct {
	RemoteChatID    string `json:"remote_chat_id"`
	UpToRemoteMsgID string `json:"up_to_remote_msg_id"`
}

type TypingParams struct {
	RemoteChatID string `json:"remote_chat_id"`
	Typing       bool   `json:"typing"`
}

type FetchMediaParams struct {
	Media    MediaRef `json:"media"`
	DestPath string   `json:"dest_path"`
}
