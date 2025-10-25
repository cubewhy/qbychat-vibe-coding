use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use sqlx::{types::Uuid, FromRow};

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterReq { pub username: String, pub password: String }

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginReq { pub username: String, pub password: String }

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResp { pub token: String, pub user: User }

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDirectChatReq { pub peer_username: String }

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Chat { pub id: Uuid, pub is_direct: bool, pub created_at: DateTime<Utc> }

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct MessageRow {
    pub id: Uuid,
    pub chat_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery { pub limit: Option<usize>, pub before: Option<DateTime<Utc>> }
