use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{types::Uuid, FromRow};

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub bio: Option<String>,
    pub is_online: bool,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub online_status_visibility: String, // "everyone", "contacts", "nobody"
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterReq {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshTokenReq {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResp {
    pub token: String,
    pub refresh_token: String,
    pub user: User,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDirectChatReq {
    pub peer_user_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateGroupReq {
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublicJoinReq {
    pub handle: String,
}



#[derive(Debug, Serialize, Deserialize)]
pub struct AddContactReq {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Contact {
    pub user_id: Uuid,
    pub contact_user_id: Uuid,
    pub status: String, // "friend", "blocked"
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ContactDto {
    pub user: User,
    pub status: String,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateChannelReq {
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransferOwnershipReq {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize, Clone)]
pub struct ChatDto {
    pub id: Uuid,
    pub r#type: String, // "direct", "group", "channel"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<SimpleUserDto>,
    pub created_at: DateTime<Utc>,
    pub member_count: i64,
    pub is_public: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_message: Option<MessageDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ParticipantDto {
    pub user_id: Uuid,
    pub username: String,
    pub joined_at: DateTime<Utc>,
    pub role: String, // "owner", "admin", "member"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<AdminPermissionsPayload>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddParticipantReq {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminPermissionsPayload {
    pub can_change_info: bool,
    pub can_delete_messages: bool,
    pub can_invite_users: bool,
    pub can_pin_messages: bool,
    pub can_manage_members: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromoteAdminReq {
    pub user_id: Uuid,
    pub permissions: Option<AdminPermissionsPayload>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminReq {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MuteReq {
    pub user_id: Uuid,
    pub minutes: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnmuteReq {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Chat {
    pub id: Uuid,
    pub is_direct: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct MessageRow {
    pub id: Uuid,
    pub chat_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<usize>,
    pub before: Option<DateTime<Utc>>,
    pub include_reads: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ForwardMessagesReq {
    pub from_chat_id: Uuid,
    pub message_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct StickerPackCreateReq {
    pub title: String,
    pub short_name: String,
}

#[derive(Debug, Deserialize)]
pub struct StickerCreateReq {
    pub file_id: Uuid,
    pub emoji: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendStickerReq {
    pub sticker_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct GifSendReq {
    pub gif_id: String,
    pub gif_url: String,
    pub gif_preview_url: String,
    pub provider: String,
}

#[derive(Debug, Deserialize)]
pub struct SetVisibilityReq {
    pub is_public: bool,
    pub public_handle: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageAttachmentDto {
    pub id: Uuid,
    pub content_type: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MessageReplyDto {
    pub id: Uuid,
    pub content: String,
    pub sender: SimpleUserDto,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MessageMentionDto {
    pub user_id: Uuid,
    pub username: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct MessageReadReceiptDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_read_by_peer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ForwardedChatDto {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SimpleUserDto {
    pub id: Uuid,
    pub username: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ForwardedFromDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat: Option<ForwardedChatDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<SimpleUserDto>,
}

#[derive(Debug, Serialize, Clone)]
pub struct StickerMessageDto {
    pub id: Uuid,
    pub pack_id: Uuid,
    pub pack_short_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    pub file_id: Uuid,
}

#[derive(Debug, Serialize, Clone)]
pub struct GifMessageDto {
    pub id: String,
    pub url: String,
    pub preview_url: String,
    pub provider: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct MessageDto {
    pub id: Uuid,
    pub chat_id: Uuid,
    pub sender: SimpleUserDto,
    pub content: String,
    pub kind: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<MessageReplyDto>,
    pub attachments: Vec<MessageAttachmentDto>,
    pub mentions: Vec<MessageMentionDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_receipt: Option<MessageReadReceiptDto>,
    pub is_pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forwarded_from: Option<ForwardedFromDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker: Option<StickerMessageDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gif: Option<GifMessageDto>,
}
