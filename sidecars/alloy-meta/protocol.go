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
	PinChats            bool    `json:"pin_chats"`
	PinMessages         bool    `json:"pin_messages"`
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
		// messagix exposes no pin API, so the sidecar keeps the pinned list on this device
		// (see chats.go); message pins have no such fallback.
		PinChats:    true,
		PinMessages: false,
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
	// AttemptsRemaining accompanies pin_rejected: how many PIN tries Meta has left.
	AttemptsRemaining *int `json:"attempts_remaining,omitempty"`
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

// Encrypted-archive history capability. These codes are the wire contract of the
// historyPin request: the host matches them instead of parsing a message.
const (
	errHistoryNoBackup  = "pin_backup_unavailable"
	errHistoryPinReject = "pin_rejected"
	errHistoryLockedOut = "pin_locked_out"
)

func errHistoryPinRejected(attemptsRemaining *int) *WireError {
	return &WireError{Type: errHistoryPinReject, AttemptsRemaining: attemptsRemaining}
}

func errHistoryPinLockedOut() *WireError {
	return &WireError{Type: errHistoryLockedOut, AttemptsRemaining: new(0)}
}

// HistoryStatus is the capability block the login payload carries: whether Meta holds a PIN
// backup for this account, and whether this sidecar has an enrolled archive device. It is
// deliberately not an error: an account without encrypted backups is a normal account.
type HistoryStatus struct {
	PinBackup bool `json:"pin_backup"`
	Enrolled  bool `json:"enrolled"`
}

// ConnectionState mirrors alloy_connector::ConnectionState. Attempt is the 1-based count of failed
// attempts in this connector's own retry loop; NextRetryMillis is the delay messagix will actually
// wait before the next attempt. Both are 0 when the state has none to report and are left off the
// wire so the host applies its own default.
type ConnectionState struct {
	Type            string  `json:"type"`
	Reason          *string `json:"reason,omitempty"`
	Attempt         uint32  `json:"attempt,omitempty"`
	NextRetryMillis *uint64 `json:"next_retry_millis,omitempty"`
}

func stateConnecting() ConnectionState { return ConnectionState{Type: "connecting"} }
func stateOnline() ConnectionState     { return ConnectionState{Type: "online"} }
func stateNeedsRelogin() ConnectionState {
	return ConnectionState{Type: "needs_relogin"}
}

func stateOffline(reason string, attempt uint32, retryAfterMs uint64) ConnectionState {
	state := ConnectionState{Type: "offline", Reason: &reason, Attempt: attempt}
	if retryAfterMs > 0 {
		state.NextRetryMillis = &retryAfterMs
	}
	return state
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
	PinOrder       *uint32   `json:"pin_order"`
	IsArchived     bool      `json:"is_archived"`
	IsMuted        bool      `json:"is_muted"`
}

// RemoteUser mirrors alloy_connector::RemoteUser.
type RemoteUser struct {
	RemoteUserID string    `json:"remote_user_id"`
	DisplayName  string    `json:"display_name"`
	Avatar       *MediaRef `json:"avatar"`
}

// RemoteMember mirrors alloy_connector::RemoteMember. Role is "admin" or absent: Messenger has
// no owner role. An empty display name means the sidecar has not seen the person's contact row.
type RemoteMember struct {
	User RemoteUser `json:"user"`
	Role *string    `json:"role"`
}

// RemoteMessage mirrors alloy_connector::RemoteMessage.
type RemoteMessage struct {
	RemoteChatID       string      `json:"remote_chat_id"`
	RemoteMsgID        string      `json:"remote_msg_id"`
	TxnID              *string     `json:"txn_id"`
	SenderRemoteID     string      `json:"sender_remote_id"`
	Sender             *RemoteUser `json:"sender"`
	IsOwn              bool        `json:"is_own"`
	SentAt             int64       `json:"sent_at"`
	EditedAt           *int64      `json:"edited_at"`
	Kind               string      `json:"kind"`
	Body               *string     `json:"body"`
	Formatted          *string     `json:"formatted"`
	ReplyToRemoteMsgID *string     `json:"reply_to_remote_msg_id"`
	Media              *MediaRef   `json:"media"`
	Pinned             bool        `json:"pinned"`
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

// ChatPinnedEvent is ConnectorEvent::ChatPinned.
type ChatPinnedEvent struct {
	Type         string `json:"type"`
	RemoteChatID string `json:"remote_chat_id"`
	Pinned       bool   `json:"pinned"`
}

func NewChatPinnedEvent(chatID string, pinned bool) ChatPinnedEvent {
	return ChatPinnedEvent{Type: "chat_pinned", RemoteChatID: chatID, Pinned: pinned}
}

// PinnedChatsReorderedEvent is ConnectorEvent::PinnedChatsReordered: the full pinned list, top
// first. The slice is never nil so the host reads an array, not null.
type PinnedChatsReorderedEvent struct {
	Type          string   `json:"type"`
	RemoteChatIDs []string `json:"remote_chat_ids"`
}

func NewPinnedChatsReorderedEvent(ids []string) PinnedChatsReorderedEvent {
	if ids == nil {
		ids = []string{}
	}
	return PinnedChatsReorderedEvent{Type: "pinned_chats_reordered", RemoteChatIDs: ids}
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
//
// Field is set only for "invalid_input", RetryAfterMs only for "rate_limited".
type LoginFailure struct {
	Type         string  `json:"type"`
	Field        *string `json:"field,omitempty"`
	Message      string  `json:"message,omitempty"`
	RetryAfterMs *uint64 `json:"retry_after_ms,omitempty"`
}

// The challenge kinds the shared contract distinguishes. A challenge is cleared on Meta's own site
// or app; the kind only decides which explanation and which affordances the UI leads with.
const (
	challengeCheckpoint = "checkpoint"
	challengeTwoFactor  = "two_factor"
	challengeConsent    = "consent"
	// challengeUnsupportedMfa is an account whose remaining sign-in methods are ones this client
	// cannot drive (a passkey, Google sign-in, or an MFA method Meta refuses to route here). Adding
	// a supported method on Meta's own site is the only way through, so the attempt stays open.
	challengeUnsupportedMfa = "unsupported_mfa"
)

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
	Kind          string        `json:"kind,omitempty"`
	Message       string        `json:"message,omitempty"`
	UsernameLabel string        `json:"username_label,omitempty"`
	Alternatives  []string      `json:"alternatives,omitempty"`
	Field         string        `json:"field,omitempty"`
	Options       []string      `json:"options,omitempty"`
	RemoteUserID  string        `json:"remote_user_id,omitempty"`
	DisplayName   string        `json:"display_name,omitempty"`
	Session       string        `json:"session,omitempty"`
	Reason        *LoginFailure `json:"reason,omitempty"`
	// History is the encrypted-archive capability block, present on the done step only.
	History *HistoryStatus `json:"history,omitempty"`
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

func doneStep(remoteUserID, displayName, session string, history *HistoryStatus) LoginStep {
	return LoginStep{
		Type:         "done",
		RemoteUserID: remoteUserID,
		DisplayName:  displayName,
		Session:      session,
		History:      history,
	}
}

func failedStep(reason LoginFailure) LoginStep {
	return LoginStep{Type: "failed", Reason: &reason}
}

// challengeStep asks the user to clear the network's own challenge. It is an ordinary step, not a
// failure: the attempt stays open, so a code answer, a method switch or a retry all still work.
func challengeStep(kind, message string) LoginStep {
	return LoginStep{Type: "challenge", Kind: kind, Message: message}
}

// waitStep says the network is waiting for the user to finish the sign-in somewhere else (approve
// the notification on their phone). The host shows the message and answers with a "continue"
// input; while the network is still waiting, the answer is this same step.
func waitStep(message string) LoginStep {
	return LoginStep{Type: "wait", Message: message}
}

// choiceStep asks the user to pick one of the network's own options (which MFA method to finish
// with, which contact point should receive the code). The answer names the field and the option
// verbatim, so the Bloks flow gets exactly the value it offered.
func choiceStep(message, field string, options []string) LoginStep {
	return LoginStep{Type: "choice", Message: message, Field: field, Options: options}
}

// LoginInput mirrors alloy_connector::LoginInput.
type LoginInput struct {
	Type     string            `json:"type"`
	Phone    string            `json:"phone,omitempty"`
	Code     string            `json:"code,omitempty"`
	Password string            `json:"password,omitempty"`
	Username string            `json:"username,omitempty"`
	To       string            `json:"to,omitempty"`
	Field    string            `json:"field,omitempty"`
	Value    string            `json:"value,omitempty"`
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

type ListMembersParams struct {
	RemoteChatID string `json:"remote_chat_id"`
}

type BackfillParams struct {
	RemoteChatID string  `json:"remote_chat_id"`
	Before       *string `json:"before"`
	Limit        uint64  `json:"limit"`
}

// HistoryPinParams carries the account's 6-digit secure-storage PIN. It is used once, inside the
// library's OPAQUE handshake, and is never logged or persisted.
type HistoryPinParams struct {
	Pin string `json:"pin"`
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

type SetChatPinnedParams struct {
	RemoteChatID string `json:"remote_chat_id"`
	Pinned       bool   `json:"pinned"`
}

type ReorderPinnedChatsParams struct {
	RemoteChatIDs []string `json:"remote_chat_ids"`
}

type SetMessagePinnedParams struct {
	RemoteChatID string `json:"remote_chat_id"`
	RemoteMsgID  string `json:"remote_msg_id"`
	Pinned       bool   `json:"pinned"`
}

type FetchMediaParams struct {
	Media    MediaRef `json:"media"`
	DestPath string   `json:"dest_path"`
}
