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

Add a participant by username. Owner or admin with `can_invite_users` can add.

Request:
{"username":"string"}

Response 200: empty
403: forbidden
404: user or chat not found

### Chat admin permission model

- Owner is immutable and implicitly holds all permissions. Only the owner can grant/revoke permissions for others.
- Admin permissions are stored per admin record. Supported boolean flags:
  - `can_change_info` — edit chat title/avatar/description, invite links.
  - `can_delete_messages` — delete any message and run `/clear_messages`.
  - `can_invite_users` — add participants to groups/channels.
  - `can_pin_messages` — pin/unpin announcements.
  - `can_manage_members` — kick, mute, or unmute other participants.
- Default: any omitted flag is treated as `false`. Posting the same admin again updates the existing permission set (idempotent). Unknown keys return 422.

### POST /api/chats/{chat_id}/admins

Promote or update an admin (owner only).

Request:
{
  "username":"string",
  "permissions":{
    "can_change_info":bool,
    "can_delete_messages":bool,
    "can_invite_users":bool,
    "can_pin_messages":bool,
    "can_manage_members":bool
  }
}

Response 200:
{
  "user_id":"uuid",
  "username":"string",
  "permissions":{...},
  "granted_by":"uuid",
  "granted_at":"RFC3339"
}

422: body missing permissions
403: requester is not owner
404: user not found or not a chat participant

### GET /api/chats/{chat_id}/admins

List owner and admins with their permissions. Requires membership.

Response 200:
{
  "owner":{
    "user_id":"uuid",
    "username":"string"
  },
  "admins":[
    {
      "user_id":"uuid",
      "username":"string",
      "permissions":{...},
      "granted_at":"RFC3339"
    }
  ]
}

### POST /api/chats/{chat_id}/admins/remove

Demote an admin. Only owner can do this.

Request:
{"username":"string"}

Response 200: empty
403: forbidden
404: user not found

### POST /api/chats/{chat_id}/remove

Remove a participant. Requires owner or admin with `can_manage_members`.

Request:
{"username":"string"}

Response 200: empty
403: forbidden
404: user not found

### POST /api/chats/{chat_id}/mute

Mute a participant for N minutes. Requires owner or admin with `can_manage_members`.

Request:
{"username":"string","minutes":30}

Response 200: empty
403: forbidden
404: user not found

### POST /api/chats/{chat_id}/unmute

Unmute a participant. Requires owner or admin with `can_manage_members`.

Request:
{"username":"string"}

Response 200: empty
403: forbidden
404: user not found

### POST /api/chats/{chat_id}/leave

Current user leaves the chat. Direct chats cannot be left (delete instead) and return 405. Owners must transfer ownership before leaving; otherwise 409.

Response 200: empty

### POST /api/chats/{chat_id}/clear_messages

Clears all messages for everyone.
- Direct chats: any participant can call.
- Groups/Channels: requires owner or admin with `can_delete_messages`.

Request: {}

Response 200: {"deleted": number}

### GET /api/chats/{chat_id}/messages?limit=50&before=RFC3339&include_reads=true|false

List messages in a chat (membership required). Messages are sorted descending by `created_at` unless `before` is used.

Response 200:
[
  {
    "id":"uuid",
    "chat_id":"uuid",
    "sender":{"id":"uuid","username":"string"},
    "content":"string",
    "created_at":"RFC3339",
    "edited_at":"RFC3339|null",
    "reply_to_message_id":"uuid|null",
    "attachments":[{"id":"uuid","content_type":"string"}],
    "mentions":[{"user_id":"uuid","username":"string"}],
    "read_receipt":{
      "read_count":12,
      "is_read_by_peer":true,
      "last_read_at":"RFC3339|null"
    },
    "is_pinned":false,
    "forwarded_from":{
      "chat":{"id":"uuid","title":"string|null"},
      "sender":{"id":"uuid","username":"string"}
    },
    "sticker":{"id":"uuid","pack_id":"uuid","pack_short_name":"string","emoji":":)","file_id":"uuid"},
    "gif":{"id":"tenor-id","url":"https://...","preview_url":"https://...","provider":"tenor"}
  }
]

- `mentions` echoes the parsed `@username` tokens.
- `read_receipt.read_count` is only present for groups/channels; `is_read_by_peer` only for direct chats; `last_read_at` is the most recent read timestamp available from either `message_reads_small`, `message_reads_agg`, or `message_views`.
- `is_pinned` is true when `message_id` equals the chat's `pinned_message_id`.
- When `include_reads=false` (default) the `read_receipt` object is omitted for bandwidth savings.

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

### Stickers & GIFs

**Sticker packs**

- POST /api/sticker_packs — create a pack. Body: {"title":"string","short_name":"unique-handle"}. Only alphanumeric/underscore/dash handles are accepted. Owner implicitly installs their pack.
- POST /api/sticker_packs/{pack_id}/stickers — add a sticker to the pack you own. Body: {"file_id":"uuid","emoji":":)"}. File IDs come from `/api/files`.
- POST /api/sticker_packs/{pack_id}/install — add an existing pack to the current user.
- DELETE /api/sticker_packs/{pack_id}/install — remove a pack from the current user.
- GET /api/me/sticker_packs — list installed packs: [{pack_id,title,short_name}].

**Sending stickers**

- POST /api/chats/{chat_id}/stickers {"sticker_id":"uuid"}
  - Requires chat membership and send permissions (channel owners only).
  - User must either own the pack or install it first; otherwise 403.
  - Creates a `message` row with `kind="sticker"` and echoes `{"id":"uuid","kind":"sticker"}`.

**GIF search & send**

- GET /api/gifs/search?q=cat&limit=20
  - Proxy to the configured provider (default Tenor). Requires the env vars: `GIF_PROVIDER_BASE_URL`, optional `GIF_PROVIDER_API_KEY`, `GIF_PROVIDER=tenor`.
  - Response: {"results":[{"id":"gif123","url":"https://...","preview_url":"https://...","provider":"tenor"},...]}
- POST /api/chats/{chat_id}/gifs {"gif_id":"gif123","gif_url":"https://...","gif_preview_url":"https://...","provider":"tenor"}
  - Validates provider matches server configuration.
  - Stores metadata (`gif_id/gif_url/gif_preview_url/gif_provider`) and returns {"id":"uuid","kind":"gif"}.

### Member Notes, Notifications & Mentions

- GET /api/chats/{chat_id}/member/note -> {"note": string|null}
- POST /api/chats/{chat_id}/member/note {"note":"string"} -> 200
- DELETE /api/chats/{chat_id}/member/note -> 200
- GET /api/chats/{chat_id}/member/notify -> {"mute_forever": bool, "mute_until": RFC3339|null, "notify_type": "all"|"mentions_only"|"none"}
- POST /api/chats/{chat_id}/member/notify {"mute_forever": bool, "mute_until": RFC3339|null, "notify_type": "all"|"mentions_only"|"none"} -> 200
- GET /api/chats/{chat_id}/member/mentions -> {"mentions":[{"message_id":"uuid","chat_id":"uuid","excerpt":"string","created_at":"RFC3339"}]}
- DELETE /api/chats/{chat_id}/member/mentions -> 200

Rules:
- Mentions are auto-created when `POST /api/chats/{chat_id}/messages` parses `@username`. Duplicate mentions per message are deduplicated.
- Notification fan-out: mention targets receive a high-priority notification even if `notify_type="mentions_only"`. Users with `notify_type="none"` never receive push, but the mention row is still stored so they can review later.
- Clearing mentions deletes all rows for the requesting member in that chat.
- Only chat members can manage their note/notification settings.

### Public discovery

- POST /api/chats/{chat_id}/visibility
  - Owner-only. Body: {"is_public":bool,"public_handle":string|null}
  - Handles must be lowercase letters/digits/`_`/`-`, 3-32 chars, unique globally. Required when `is_public=true` for groups/channels.
- GET /api/chats/public_search?handle=rust
  - Authenticated users can search public chats by prefix. Returns {"results":[{"id","title","public_handle","chat_type"},...]}
- POST /api/chats/public_join {"handle":"rustaceans"}
  - Join a public group/channel by handle. Fails with 403 when the chat isn't public or is a direct chat.

### Messages

- POST /api/chats/{chat_id}/messages
  - Send a message in a chat you joined. Returns {"id":"uuid"}
  - Request fields:
    - content: string
    - attachment_ids: [uuid] — optional, references uploaded files for images/videos/voice/files
    - reply_to_message_id: uuid — optional, reply to an existing message in same chat
  - Mention handling:
    - Server parses `content` for tokens matching `@{username}` (case-insensitive, letters/digits/underscores).
    - For each mentioned participant, a row is inserted into `member_mentions`.
    - Up to 50 unique mentions per message; exceeding this returns 422.
    - Mentioned users bypass muted state unless `notify_type="none"`.
    - The response includes a `mentions` array mirroring what `GET /messages` returns.
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
- POST /api/chats/{chat_id}/forward_messages
  - Request: {"from_chat_id":"uuid","message_ids":["uuid",...]}
  - Requirements:
    - Caller must be a member of both source and target chats.
    - Copies each message's content/attachments/sticker/gif into the target chat.
    - Inserts metadata (`forward_from_chat_id`, `forward_from_message_id`, `forward_from_sender_id`) so clients can render "Forwarded from ..." headers.
  - Response: {"message_ids":["uuid",...]} in the same order as the request.
  - Bad paths:
    - 403 when not a member of source chat.
    - 400 when any message_id does not belong to from_chat_id or is deleted.
- GET /api/messages/{message_id}/reads?limit=50&cursor=uuid
  - Returns the members who read a specific group/channel message.
  - Response 200:
    {
      "message_id":"uuid",
      "readers":[{"user_id":"uuid","username":"string","read_at":"RFC3339"}],
      "next_cursor":"uuid|null"
    }
  - Only available for groups with <=100 participants. Caller must be the sender, the owner, or have `can_delete_messages`. Direct chats/channels use aggregate counters instead.
- Admin
  - POST /api/admin/reads/purge: delete message_reads_small older than 7 days

### Chat list
- GET /api/chats?include_unread=true|false&include_first=true|false
  - Returns all chats for current user.
  - Optional fields when requested:
    - unread: unread count computed by last_read_message_id timestamp and excluding deleted messages
    - first_message: earliest non-deleted message in the chat
  - Always includes `is_public`, `public_handle`, and when present, `pinned_message` (lightweight message object without read receipts).

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

### Pinned Messages

- Schema: `chats.pinned_message_id uuid|null`. Null indicates no pinned announcement.
- Chat payloads:
  - `GET /api/chats?...` includes `"pinned_message"` when present (same shape as `GET /messages` but without `read_receipt` to keep payload small).
  - `GET /api/chats/{chat_id}/messages` exposes `is_pinned=true` on the pinned item.
- POST /api/chats/{chat_id}/pin_message
  - Request: {"message_id":"uuid"}
  - Authorization: owner or admin with `can_pin_messages`.
  - Validates message belongs to chat and is not deleted. Returns 200 with {"pinned_message_id":"uuid"} after updating.
  - Idempotent: pinning the current message again is a no-op.
- POST /api/chats/{chat_id}/unpin_message
  - Authorization: same as pin.
  - Clears `pinned_message_id` and returns 200 with {"pinned_message_id":null}.

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

### WebSocket Real-Time Experience Module Detailed Design Specification

#### I. Core Concept

The current WebSocket is used for one-way broadcasting of new messages. We will extend it to enable bidirectional communication and handle various real-time events. The core concept is: **Any state update that may cause changes in the user interface (UI) should be pushed in real-time to relevant clients via WebSocket, avoiding clients polling HTTP APIs to refresh status.**

This includes:
1. **Typing status**: "The other party is typing..."
2. **Message status changes**: Messages being edited or deleted.
3. **Read receipts**: "Double check" in private chats, read count updates in group chats.
4. **User online status**: Online status of friends/chat members ("online" or "last seen time").
5. **Chat metadata changes**: Messages being pinned, group info being modified, etc.

#### II. WebSocket Message Structure

We will unify the message format for client and server, all messages are JSON objects containing `type` and `payload` fields.

```json
{
  "type": "string (event type)",
  "payload": { ... } // object (specific data of the event)
}
```

#### III. Client -> Server (C2S) Events

Clients need to actively report some instantaneous statuses to the server.

##### 1. `start_typing`
Sent when the user starts typing in the input box of a chat window.

* **`type`**: `"start_typing"`
* **`payload`**:
    ```json
    {
      "chat_id": "uuid"
    }
    ```
* **Server logic**:
    1. Upon receipt, record the user's typing status in this `chat_id`, and set a 5-second timeout.
    2. Broadcast a `typing_indicator` event to **other** online members in this chat.
    3. If another `start_typing` from the same user is received within 5 seconds, reset the timeout.
    4. If timeout, consider the user has stopped typing, can broadcast a stop event (optional, better to let client handle timeout).

##### 2. `mark_as_read`
Sent when the user's viewport scrolls to a message, marking it as "read". This is more real-time than `read_bulk` HTTP API.

* **`type`**: `"mark_as_read"`
* **`payload`**:
    ```json
    {
      "chat_id": "uuid",
      "last_read_message_id": "uuid" // The ID of the latest message visible in the user's viewport
    }
    ```
* **Server logic**:
    1. Verify the user is a member of this chat.
    2. Update the user's `chat_members.last_read_message_id` in the database.
    3. Trigger a `messages_read` event, broadcast to required clients (see below).

#### IV. Server -> Client (S2C) Events

This is the core of real-time experience, the server needs to push various types of events to clients based on different business logic.

##### 1. `new_message` (replaces the original `message`)
Broadcast when there is a new message.

* **`type`**: `"new_message"`
* **`payload`**:
    * Complete message object, structure consistent with a single message returned by `GET /api/chats/{chat_id}/messages`.

##### 2. `typing_indicator`
Broadcast user's typing status.

* **`type`**: `"typing_indicator"`
* **`payload`**:
    ```json
    {
      "chat_id": "uuid",
      "user": {
        "id": "uuid",
        "username": "string"
      }
    }
    ```
* **Client logic**: Upon receipt, display "username is typing..." in the corresponding chat window title bar or at the bottom of the message list, and set a 6-second timer, hide the prompt automatically after timeout.

##### 3. `message_edited`
Broadcast when a message is edited.

* **`type`**: `"message_edited"`
* **`payload`**:
    * **Complete, updated message object**. This allows the client to directly replace the old message in local cache without reassembly.

##### 4. `message_deleted`
Broadcast when one (or more) messages are deleted.

* **`type`**: `"message_deleted"`
* **`payload`**:
    ```json
    {
      "chat_id": "uuid",
      "message_ids": ["uuid", "uuid", ...]
    }
    ```
* **Client logic**: Find these messages in local data, and handle according to the app's UI/UX rules (e.g., replace content with "[Message deleted]" or remove directly).

##### 5. `messages_read`
Pushed when a user's read status is updated. This is the most complex event, requiring differentiation of push targets based on scenarios.

* **`type`**: `"messages_read"`
* **`payload`**:
    ```json
    {
      "chat_id": "uuid",
      "reader_user_id": "uuid", // Who read
      "last_read_message_id": "uuid", // The latest message ID this user has read
      "read_count": 13, // (Groups only) New total read count for this message
      "is_read_by_peer": true // (Private chats only) Whether the peer has read
    }
    ```
* **Push logic**:
    * **Scenario A: Private chat**
        * When user A reads messages in private chat with B, server **pushes this event only to all online devices of user B**.
    * **Scenario B: Group chat**
        * When user A reads messages in group chat, server **pushes this event only to the message sender**, to update the read count on sender's UI.
    * **Scenario C: User's own multi-device sync**
        * When user A reads messages on device 1, server needs to **push this event to user A's other online devices (device 2, 3)**, so they can sync to clear unread badges.

##### 6. `presence_update`
Broadcast user's online status changes.

* **`type`**: `"presence_update"`
* **`payload`**:
    ```json
    {
      "user_id": "uuid",
      "status": "online" | "offline",
      "last_seen_at": "RFC3339|null" // Provided when status is offline
    }
    ```
* **Push logic**:
    * Requires an online status service (usually implemented with Redis).
    * When user WebSocket connection is established, mark as `online`, and broadcast `presence_update` to all online users who have **common chats** with them.
    * When user WebSocket disconnects (needs heartbeat and timeout mechanism), mark as `offline` and record `last_seen_at`, then broadcast.

##### 7. `chat_action`
A general chat action event, for handling non-message updates.

* **`type`**: `"chat_action"`
* **`payload`**:
    ```json
    {
      "chat_id": "uuid",
      "action_type": "message_pinned" | "chat_info_updated" | "user_joined" | "user_left",
      "data": { ... } // Data related to the action
    }
    ```
* **`data` examples**:
    * For `message_pinned`, `data` can be `{ "pinned_message": MessageObject }`.
    * For `chat_info_updated`, `data` can be `{ "new_title": "New Title" }`.
    * For `user_joined`, `data` can be `{ "user": { "id": "uuid", "username": "string" } }`.

---

#### V. HTTP API and WebSocket Integration

Now, after completing database operations, your HTTP API also needs to trigger corresponding WebSocket events.

* **`POST /api/messages/{message_id}/edit`**: After success, broadcast `message_edited` event to this `chat_id`.
* **`POST /api/messages/{message_id}/delete`**: After success, broadcast `message_deleted` event to this `chat_id`.
* **`POST /api/chats/{chat_id}/pin_message`**: After success, broadcast `chat_action` (type: `message_pinned`) event to this `chat_id`.
* **`POST /api/chats/{chat_id}/remove`**: After success, broadcast `chat_action` (type: `user_left`) event to this `chat_id`.
