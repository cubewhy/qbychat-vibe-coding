# QbyChat API Spec

Base URL: http://localhost:8080
Auth: Bearer JWT in Authorization header. For WebSocket, pass ?token=... or Authorization header.

## Auth

### POST /api/register

Request:
{
"username": "string",
"password": "string (>=6)"
}

Response 200:
{
"token": "jwt",
"user": {"id":"uuid","username":"string","created_at":"RFC3339"}
}

409: username exists
400: invalid payload

### POST /api/login

Request:
{"username":"string","password":"string"}

Response 200: same as /api/register
401: unauthorized

## Chats

### POST /api/chats/direct

Create or get a direct chat with another user by username.

Request:
{"peer_username":"string"}

Response 200:
{"chat_id":"uuid"}

404: peer not found

### POST /api/chats/group

Create a group chat. Creator becomes owner and first participant.

Request:
{"title":"string"}

Response 200:
{"chat_id":"uuid"}

### POST /api/chats/channel

Create a broadcast channel. Only owner can send messages.

Request:
{"title":"string"}

Response 200:
{"chat_id":"uuid"}

### POST /api/chats/{chat_id}/participants

Add a participant by username. Only owner can add.

Request:
{"username":"string"}

Response 200: empty
403: forbidden
404: user or chat not found

### POST /api/chats/{chat_id}/admins

Promote a user to admin. Only owner can do this.

Request:
{"username":"string"}

Response 200: empty
403: forbidden
404: user not found

### POST /api/chats/{chat_id}/admins/remove

Demote an admin. Only owner can do this.

Request:
{"username":"string"}

Response 200: empty
403: forbidden
404: user not found

### POST /api/chats/{chat_id}/remove

Remove a participant. Owner or admin can do this.

Request:
{"username":"string"}

Response 200: empty
403: forbidden
404: user not found

### POST /api/chats/{chat_id}/mute

Mute a participant for N minutes. Owner or admin can do this.

Request:
{"username":"string","minutes":30}

Response 200: empty
403: forbidden
404: user not found

### POST /api/chats/{chat_id}/unmute

Unmute a participant. Owner or admin can do this.

Request:
{"username":"string"}

Response 200: empty
403: forbidden
404: user not found

### GET /api/chats/{chat_id}/messages?limit=50&before=RFC3339

List messages in a chat (requires membership).

Response 200:
[
{"id":"uuid","chat_id":"uuid","sender_id":"uuid","content":"string","created_at":"RFC3339"}
]

403: not a participant

## WebSocket

Path: /ws?token=...

Client -> Server messages (JSON):

- {"type":"send_message","chat_id":"uuid","content":"string"}

Server -> Client messages (JSON):

- {"type":"message","message": Message}
- {"type":"error","message": string}

Notes:

- Server broadcasts message to all chat participants with active WS connections.
- Use HTTP API to fetch history.
