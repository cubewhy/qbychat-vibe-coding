use crate::auth::{conflict_or_internal, internal_err, make_token};
use crate::models::{AuthResp, LoginReq, RefreshTokenReq, RegisterReq, User};
use crate::state::AppState;
use actix_web::{post, web, HttpResponse};
use sqlx::types::Uuid;

#[post("/v1/api/register")]
pub async fn register(
    state: web::Data<AppState>,
    payload: web::Json<RegisterReq>,
) -> actix_web::Result<HttpResponse> {
    let username = payload.username.trim();
    if username.is_empty() || payload.password.len() < 6 {
        return Ok(HttpResponse::BadRequest().body("invalid payload"));
    }
    let user_id = Uuid::new_v4();
    let password_hash = hash_password(&payload.password).map_err(internal_err)?;
    let rec = sqlx::query_as::<_, User>(
        r#"INSERT INTO users (id, username, password_hash, bio, is_online, last_seen_at, online_status_visibility)
           VALUES ($1, $2, $3, NULL, false, NULL, 'everyone')
           RETURNING id, username, bio, is_online, last_seen_at, online_status_visibility, created_at"#,
    )
    .bind(user_id)
    .bind(username)
    .bind(password_hash)
    .fetch_one(&state.pool)
    .await
    .map_err(conflict_or_internal)?;

    let token = make_token(rec.id, &state.jwt_secret)?;
    let refresh_token_str = Uuid::new_v4().to_string();
    // Store refresh_token in Redis with TTL
    if let Some(redis) = &state.redis {
        let mut conn = redis.get_async_connection().await.map_err(internal_err)?;
        let _: () = redis::cmd("SETEX").arg(&format!("refresh:{}", refresh_token_str)).arg(60*60*24*30).arg(rec.id.to_string()).query_async(&mut conn).await.map_err(internal_err)?;
    }
    Ok(HttpResponse::Ok().json(AuthResp { token, refresh_token: refresh_token_str, user: rec }))
}

#[post("/v1/api/login")]
pub async fn login(
    state: web::Data<AppState>,
    payload: web::Json<LoginReq>,
) -> actix_web::Result<HttpResponse> {
    #[derive(sqlx::FromRow)]
    struct LoginRow {
        id: Uuid,
        username: String,
        bio: Option<String>,
        is_online: bool,
        last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
        online_status_visibility: String,
        created_at: chrono::DateTime<chrono::Utc>,
        password_hash: String,
    }
    let maybe = sqlx::query_as::<_, LoginRow>(
        "SELECT id, username, bio, is_online, last_seen_at, online_status_visibility, created_at, password_hash FROM users WHERE username = $1",
    )
    .bind(payload.username.trim())
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?;

    let Some(row) = maybe else {
        return Ok(HttpResponse::Unauthorized().finish());
    };
    if !verify_password(&payload.password, &row.password_hash).map_err(internal_err)? {
        return Ok(HttpResponse::Unauthorized().finish());
    }
    let user = User {
        id: row.id,
        username: row.username,
        bio: row.bio,
        is_online: row.is_online,
        last_seen_at: row.last_seen_at,
        online_status_visibility: row.online_status_visibility,
        created_at: row.created_at,
    };
    let token = make_token(user.id, &state.jwt_secret)?;
    let refresh_token_str = Uuid::new_v4().to_string();
    if let Some(redis) = &state.redis {
        let mut conn = redis.get_async_connection().await.map_err(internal_err)?;
        let _: () = redis::cmd("SETEX").arg(&format!("refresh:{}", refresh_token_str)).arg(60*60*24*30).arg(user.id.to_string()).query_async(&mut conn).await.map_err(internal_err)?;
    }
    Ok(HttpResponse::Ok().json(AuthResp { token, refresh_token: refresh_token_str, user }))
}

fn hash_password(pw: &str) -> anyhow::Result<String> {
    use argon2::password_hash::SaltString;
    use argon2::{Argon2, PasswordHasher};
    use rand::RngCore;
    let mut salt_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)?;
    let argon2 = Argon2::default();
    Ok(argon2.hash_password(pw.as_bytes(), &salt)?.to_string())
}

#[post("/v1/api/refresh_token")]
pub async fn refresh_token(
    state: web::Data<AppState>,
    payload: web::Json<RefreshTokenReq>,
) -> actix_web::Result<HttpResponse> {
    let refresh_token = &payload.refresh_token;
    let user_id_str: Option<String> = if let Some(redis) = &state.redis {
        let mut conn = redis.get_async_connection().await.map_err(internal_err)?;
        redis::cmd("GET").arg(&format!("refresh:{}", refresh_token)).query_async(&mut conn).await.map_err(internal_err)?
    } else {
        return Ok(HttpResponse::Unauthorized().finish());
    };
    let Some(user_id_str) = user_id_str else {
        return Ok(HttpResponse::Unauthorized().finish());
    };
    let user_id = Uuid::parse_str(&user_id_str).map_err(internal_err)?;
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, bio, is_online, last_seen_at, online_status_visibility, created_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(internal_err)?;
    let token = make_token(user.id, &state.jwt_secret)?;
    let new_refresh_token_str = Uuid::new_v4().to_string();
    if let Some(redis) = &state.redis {
        let mut conn = redis.get_async_connection().await.map_err(internal_err)?;
        let _: () = redis::cmd("SETEX").arg(&format!("refresh:{}", new_refresh_token_str)).arg(60*60*24*30).arg(user.id.to_string()).query_async(&mut conn).await.map_err(internal_err)?;
        // Optionally delete old refresh token
        let _: () = redis::cmd("DEL").arg(&format!("refresh:{}", refresh_token)).query_async(&mut conn).await.map_err(internal_err)?;
    }
    Ok(HttpResponse::Ok().json(AuthResp { token, refresh_token: new_refresh_token_str, user }))
}

fn verify_password(pw: &str, hash: &str) -> anyhow::Result<bool> {
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(pw.as_bytes(), &parsed)
        .is_ok())
}
