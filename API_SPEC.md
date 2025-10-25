# QbyChat API Spec

Base URL: http://localhost:8080
Auth: Bearer JWT in Authorization header. For WebSocket, pass ?token=... or Authorization header.

This project aims to be a lightweight Telegram-like clone.

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

### Avatars & Files

- POST /api/users/me/avatars (multipart)
  - Upload one or multiple images. Returns uploaded avatar list.
  - Query: compress=true|false (default false), quality=1..100 (default 80). When compressing images, EXIF is stripped.
- POST /api/users/me/avatars/primary
  - Set primary avatar: {"avatar_id":"uuid"}
- GET /api/users/{user_id}/avatars
  - List all avatars of user: [{id, content_type, is_primary, created_at}]
- POST /api/files (multipart)
  - Generic file upload with de-duplication by sha256. Query: compress=true|false, quality=1..100. Image files will be compressed and EXIF stripped when compress=true. If the uploaded content already exists (same sha256), the existing file id is returned. Returns [{id, content_type, created_at}]
- POST /api/files/download_token
  - Request a short-lived token to download an avatar file: {"avatar_id":"uuid"}
  - Returns {"token":"string","expires_at":"RFC3339"}
- GET /api/files/{token}
  - Download file using token. Token expires (default 30min via DOWNLOAD_TOKEN_TTL_SECS). Requires Redis.
- POST /api/admin/storage/purge
  - Purge unreferenced storage files (no rows in message_attachments). Auth: X-Admin-Token header must equal env ADMIN_TOKEN. Response: {"deleted": number}
- Cron purge
  - Deploy an external cron (e.g., host or container) to call POST /api/admin/storage/purge daily at 00:00.

### Member Notes
- GET /api/chats/{chat_id}/member/note -> {"note": string|null}
- POST /api/chats/{chat_id}/member/note {"note":"string"} -> 200
- DELETE /api/chats/{chat_id}/member/note -> 200
  - Only chat members can manage their note.

### Chat administration
- POST /api/chats/{chat_id}/remove — kick a member (owner/admin only)
- POST /api/chats/{chat_id}/leave — current user leaves the chat
- POST /api/chats/{chat_id}/clear_messages —
  - Direct chats: any participant can clear all messages
  - Groups/Channels: only owner or admins can clear all messages

### Messages

- POST /api/chats/{chat_id}/messages
  - Send a message in a chat you joined. Request: {"content":"string"}. Returns {"id":"uuid"}
- POST /api/messages/{message_id}/edit
  - Edit own message. Request: {"content":"string"}. 403 if not owner or message deleted. Sets edited_at.
- POST /api/messages/{message_id}/delete
  - Soft delete own message. Sets is_deleted=true, deleted_at=now(). Listing messages will return empty content for deleted ones.
- POST /api/messages/read_bulk
  - Bulk mark messages as read. Request: {"chat_id":"uuid","message_ids":["uuid",...]}
  - Rules:
    - channel: increments message_views.views and updates last_view_at
    - direct or group with participants > 100: set message_reads_agg.is_read=true and first_read_at if null
    - group with participants <= 100: upsert message_reads_small(message_id,user_id,read_at=now())
- Admin
  - POST /api/admin/reads/purge: delete message_reads_small older than 7 days

### Members & Unread

- Member entity: chat_members(chat_id, user_id, note, last_read_message_id, created_at)
  - Creation time: when a user joins a chat (direct/group/channel), a row is inserted.
  - last_read_message_id is used to compute unread counts by time; deleted messages are not counted.
- GET /api/chats/{chat_id}/unread_count
  - Returns {"unread": number}
  - Logic:
    - Take T = created_at of last_read_message_id; if null, count from beginning
    - Count messages in the chat with created_at > T AND is_deleted=false
- Bulk read advances chat_members.last_read_message_id to the newest message in the batch

### Member Notes
- GET /api/chats/{chat_id}/member/note -> {"note": string|null}
- POST /api/chats/{chat_id}/member/note {"note":"string"} -> 200
- DELETE /api/chats/{chat_id}/member/note -> 200
  - Only chat members can manage their note.

### Chat administration
- POST /api/chats/{chat_id}/remove — kick a member (owner/admin only)
- POST /api/chats/{chat_id}/leave — current user leaves the chat
- POST /api/chats/{chat_id}/clear_messages —
  - Direct chats: any participant can clear all messages
  - Groups/Channels: only owner or admins can clear all messages

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
