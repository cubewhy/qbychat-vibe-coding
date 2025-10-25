use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use actix_web::{get, post};
use actix_web::http::header::HeaderMap;
use actix_ws::{Message, MessageStream, Session};
use anyhow::Context;
use chrono::{DateTime, Utc};
use dotenvy::dotenv;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Pool, Postgres};
use sqlx::types::Uuid;
use sqlx::FromRow;
use std::sync::Arc;
use tokio::sync::mpsc;
use dashmap::DashMap;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    // user_id -> tx for outgoing ws messages
    clients: Arc<DashMap<Uuid, mpsc::UnboundedSender<ServerWsMsg>>>,
    jwt_secret: Arc<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
struct User {
    id: Uuid,
    username: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterReq { username: String, password: String }

#[derive(Debug, Serialize, Deserialize)]
struct LoginReq { username: String, password: String }

#[derive(Debug, Serialize, Deserialize)]
struct AuthResp { token: String, user: User }

#[derive(Debug, Serialize, Deserialize)]
struct CreateDirectChatReq { peer_username: String }

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct Chat { id: Uuid, is_direct: bool, created_at: DateTime<Utc> }

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
struct MessageRow {
    id: Uuid,
    chat_id: Uuid,
    sender_id: Uuid,
    content: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ClientWsMsg {
    #[serde(rename = "send_message")]
    SendMessage { chat_id: Uuid, content: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
enum ServerWsMsg {
    #[serde(rename = "message")]
    Message { message: MessageRow },
    #[serde(rename = "error")]
    Error { message: String },
}

#[post("/api/register")]
async fn register(state: web::Data<AppState>, payload: web::Json<RegisterReq>) -> actix_web::Result<HttpResponse> {
    let username = payload.username.trim();
    if username.is_empty() || payload.password.len() < 6 {
        return Ok(HttpResponse::BadRequest().body("invalid payload"));
    }
    let user_id = Uuid::new_v4();
    let password_hash = hash_password(&payload.password).map_err(internal_err)?;
    let rec = sqlx::query_as::<_, User>(
        r#"INSERT INTO users (id, username, password_hash)
           VALUES ($1, $2, $3)
           RETURNING id, username, created_at"#,
    )
    .bind(user_id)
    .bind(username)
    .bind(password_hash)
    .fetch_one(&state.pool)
    .await
    .map_err(conflict_or_internal)?;

    let token = make_token(rec.id, &state.jwt_secret)?;
    Ok(HttpResponse::Ok().json(AuthResp { token, user: rec }))
}

#[post("/api/login")]
async fn login(state: web::Data<AppState>, payload: web::Json<LoginReq>) -> actix_web::Result<HttpResponse> {
    #[derive(FromRow)]
    struct LoginRow { id: Uuid, username: String, created_at: DateTime<Utc>, password_hash: String }
    let maybe = sqlx::query_as::<_, LoginRow>(
        "SELECT id, username, created_at, password_hash FROM users WHERE username = $1"
    )
    .bind(payload.username.trim())
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?;

    let Some(row) = maybe else { return Ok(HttpResponse::Unauthorized().finish()); };
    if !verify_password(&payload.password, &row.password_hash).map_err(internal_err)? {
        return Ok(HttpResponse::Unauthorized().finish());
    }
    let user = User { id: row.id, username: row.username, created_at: row.created_at };
    let token = make_token(user.id, &state.jwt_secret)?;
    Ok(HttpResponse::Ok().json(AuthResp { token, user }))
}

#[post("/api/chats/direct")]
async fn start_direct_chat(state: web::Data<AppState>, req: web::Json<CreateDirectChatReq>, user: AuthUser) -> actix_web::Result<HttpResponse> {
    // find peer
    #[derive(FromRow)]
    struct IdRow { id: Uuid }
    let peer = sqlx::query_as::<_, IdRow>("SELECT id FROM users WHERE username = $1")
        .bind(req.peer_username.trim())
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("peer not found"))?;

    let chat_id = ensure_direct_chat(&state.pool, user.0, peer.id).await.map_err(internal_err)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "chat_id": chat_id })))
}

#[get("/api/chats/{chat_id}/messages")]
async fn list_messages(state: web::Data<AppState>, path: web::Path<Uuid>, user: AuthUser, q: web::Query<ListQuery>) -> actix_web::Result<HttpResponse> {
    let chat_id = path.into_inner();
    // ensure membership
    let membership = sqlx::query(
        "SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2"
    )
    .bind(chat_id)
    .bind(user.0)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?;
    if membership.is_none() { return Ok(HttpResponse::Forbidden().finish()); }

    let limit = q.limit.unwrap_or(50).min(200) as i64;
    let rows = if let Some(before) = q.before {
        sqlx::query_as::<_, MessageRow>(
            r#"SELECT id, chat_id, sender_id, content, created_at
               FROM messages
               WHERE chat_id = $1 AND created_at < $2
               ORDER BY created_at DESC
               LIMIT $3"#,
        )
        .bind(chat_id)
        .bind(before)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(internal_err)?
    } else {
        sqlx::query_as::<_, MessageRow>(
            r#"SELECT id, chat_id, sender_id, content, created_at
               FROM messages
               WHERE chat_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#,
        )
        .bind(chat_id)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(internal_err)?
    };

    Ok(HttpResponse::Ok().json(rows))
}

#[derive(Debug, Deserialize)]
struct ListQuery { limit: Option<usize>, before: Option<DateTime<Utc>> }

#[get("/ws")]
async fn ws_route(state: web::Data<AppState>, req: actix_web::HttpRequest, body: web::Payload) -> actix_web::Result<impl Responder> {
    // token from query or Authorization
    let token = req.uri().query().and_then(|q| {
        web::Query::<std::collections::HashMap<String, String>>::from_query(q).ok()?.get("token").cloned()
    }).or_else(|| extract_bearer(req.headers()).ok());

    let Some(token) = token else { return Ok(HttpResponse::Unauthorized().finish()); };
    let user_id = decode_token(&token, &state.jwt_secret).map_err(|_| actix_web::error::ErrorUnauthorized("invalid token"))?;

    let (res, session, stream) = actix_ws::handle(&req, body)?;
    let state_cloned = state.get_ref().clone();
    actix_web::rt::spawn(ws_session(state_cloned, user_id, session, stream));
    Ok(res)
}

async fn ws_session(state: AppState, user_id: Uuid, mut session: Session, mut stream: MessageStream) {
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerWsMsg>();
    state.clients.insert(user_id, tx);
    info!(%user_id, "ws connected");

    // task to forward outgoing messages to socket
    let mut closed = false;
    while !closed {
        tokio::select! {
            Some(msg) = stream.next() => {
                match msg {
                    Ok(Message::Text(txt)) => {
                        if let Err(e) = handle_ws_text(&state, user_id, &txt).await { error!(%user_id, ?e, "handle ws text error"); }
                    }
                    Ok(Message::Close(_)) => { closed = true; }
                    Ok(Message::Ping(data)) => { let _ = session.pong(&data); }
                    Ok(Message::Pong(_)) => {}
                    Ok(Message::Binary(_)) => {}
                    Ok(Message::Continuation(_)) | Ok(Message::Nop) => {}
                    Err(e) => { error!(?e, "ws error"); closed = true; }
                }
            }
            Some(server_msg) = rx.recv() => {
                let txt = serde_json::to_string(&server_msg).unwrap_or_else(|_| "{\"type\":\"error\",\"message\":\"serialization\"}".into());
                if session.text(txt).await.is_err() { closed = true; }
            }
            else => { closed = true; }
        }
    }

    state.clients.remove(&user_id);
    info!(%user_id, "ws disconnected");
}

async fn handle_ws_text(state: &AppState, user_id: Uuid, txt: &str) -> anyhow::Result<()> {
    let msg: ClientWsMsg = serde_json::from_str(txt)?;
    match msg {
        ClientWsMsg::SendMessage { chat_id, content } => {
            // ensure membership
            let is_member = sqlx::query(
                "SELECT 1 FROM chat_participants WHERE chat_id = $1 AND user_id = $2"
            ).bind(chat_id).bind(user_id).fetch_optional(&state.pool).await?;
            if is_member.is_none() { send_ws_err(state, user_id, "not a member"); return Ok(()); }

            let mid = Uuid::new_v4();
            let saved = sqlx::query_as::<_, MessageRow>(
                r#"INSERT INTO messages (id, chat_id, sender_id, content)
                   VALUES ($1, $2, $3, $4)
                   RETURNING id, chat_id, sender_id, content, created_at"#,
            )
            .bind(mid)
            .bind(chat_id)
            .bind(user_id)
            .bind(content)
            .fetch_one(&state.pool)
            .await?;

            // broadcast to all participants
            #[derive(FromRow)]
            struct UserIdRow { user_id: Uuid }
            let participants = sqlx::query_as::<_, UserIdRow>(
                "SELECT user_id FROM chat_participants WHERE chat_id = $1"
            ).bind(chat_id).fetch_all(&state.pool).await?;

            let server_msg = ServerWsMsg::Message { message: saved.clone() };
            for p in participants {
                if let Some(tx) = state.clients.get(&p.user_id) { let _ = tx.send(server_msg.clone()); }
            }
        }
    }
    Ok(())
}

fn send_ws_err(state: &AppState, user_id: Uuid, msg: &str) {
    if let Some(tx) = state.clients.get(&user_id) { let _ = tx.send(ServerWsMsg::Error { message: msg.to_string() }); }
}

#[derive(Clone)]
struct AuthUser(Uuid);

impl actix_web::FromRequest for AuthUser {
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &actix_web::HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let headers = req.headers().clone();
        let query = req.uri().query().map(|s| s.to_string());
        let secret = req.app_data::<web::Data<AppState>>().unwrap().jwt_secret.clone();
        Box::pin(async move {
            let token = if let Some(q) = query {
                if let Ok(qmap) = web::Query::<std::collections::HashMap<String, String>>::from_query(&q) {
                    qmap.get("token").cloned()
                } else { None }
            } else { None };
            let token = token.or_else(|| extract_bearer(&headers).ok());
            let token = token.ok_or_else(|| actix_web::error::ErrorUnauthorized("missing token"))?;
            let uid = decode_token(&token, &secret).map_err(|_| actix_web::error::ErrorUnauthorized("invalid token"))?;
            Ok(AuthUser(uid))
        })
    }
}

fn extract_bearer(headers: &HeaderMap) -> Result<String, ()> {
    let Some(value) = headers.get(actix_web::http::header::AUTHORIZATION) else { return Err(()); };
    let Ok(s) = value.to_str() else { return Err(()); };
    if let Some(rest) = s.strip_prefix("Bearer ") { Ok(rest.to_string()) } else { Err(()) }
}

#[derive(Serialize, Deserialize)]
struct Claims { sub: String, exp: usize }

fn make_token(user_id: Uuid, secret: &str) -> actix_web::Result<String> {
    use jsonwebtoken::{encode, Header, EncodingKey};
    let exp = (Utc::now() + chrono::Duration::days(30)).timestamp() as usize;
    let claims = Claims { sub: user_id.to_string(), exp };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).map_err(internal_err)
}

fn decode_token(token: &str, secret: &str) -> Result<Uuid, jsonwebtoken::errors::Error> {
    use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
    let data = decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::new(Algorithm::HS256))?;
    Ok(Uuid::parse_str(&data.claims.sub).expect("valid uuid in token"))
}

fn hash_password(pw: &str) -> anyhow::Result<String> {
    use argon2::{Argon2, PasswordHasher};
    use rand::RngCore;
    use argon2::password_hash::SaltString;
    let mut salt_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)?;
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(pw.as_bytes(), &salt)?.to_string();
    Ok(hash)
}

fn verify_password(pw: &str, hash: &str) -> anyhow::Result<bool> {
    use argon2::{Argon2, PasswordVerifier};
    use argon2::password_hash::{PasswordHash, PasswordVerifier as _};
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default().verify_password(pw.as_bytes(), &parsed).is_ok())
}

async fn ensure_direct_chat(pool: &Pool<Postgres>, a: Uuid, b: Uuid) -> anyhow::Result<Uuid> {
    // check existing chat with both participants and exactly 2 members
    #[derive(FromRow)]
    struct ChatIdRow { id: Uuid }
    if let Some(row) = sqlx::query_as::<_, ChatIdRow>(
        r#"SELECT c.id
           FROM chats c
           JOIN chat_participants p1 ON p1.chat_id = c.id AND p1.user_id = $1
           JOIN chat_participants p2 ON p2.chat_id = c.id AND p2.user_id = $2
           WHERE c.is_direct = TRUE
           LIMIT 1"#,
    ).bind(a).bind(b).fetch_optional(pool).await? { return Ok(row.id); }

    let chat_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO chats (id, is_direct) VALUES ($1, TRUE)").bind(chat_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO chat_participants (chat_id, user_id) VALUES ($1, $2), ($1, $3)").bind(chat_id).bind(a).bind(b).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(chat_id)
}

fn internal_err<E: std::fmt::Debug>(e: E) -> actix_web::Error { actix_web::error::ErrorInternalServerError(format!("{:?}", e)) }
fn conflict_or_internal<E: std::fmt::Debug>(e: E) -> actix_web::Error {
    actix_web::error::ErrorConflict(format!("{:?}", e))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL not set")?;
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "devsecretchangeme".into());

    let pool = PgPool::connect(&database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = AppState { pool, clients: Arc::new(DashMap::new()), jwt_secret: Arc::new(jwt_secret) };

    info!("listening on {}", bind_addr);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .service(register)
            .service(login)
            .service(start_direct_chat)
            .service(list_messages)
            .service(ws_route)
            .default_service(web::route().to(|| async { HttpResponse::NotFound().finish() }))
    })
    .bind(bind_addr)?
    .workers(1) // use single worker so ws futures can be spawned with spawn_local
    .run()
    .await?;

    Ok(())
}
