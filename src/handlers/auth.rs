use crate::auth::{conflict_or_internal, internal_err, make_token};
use crate::models::{AuthResp, LoginReq, RegisterReq, User};
use crate::state::AppState;
use actix_web::{post, web, HttpResponse};
use sqlx::types::Uuid;

#[post("/api/register")]
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
pub async fn login(
    state: web::Data<AppState>,
    payload: web::Json<LoginReq>,
) -> actix_web::Result<HttpResponse> {
    #[derive(sqlx::FromRow)]
    struct LoginRow {
        id: Uuid,
        username: String,
        created_at: chrono::DateTime<chrono::Utc>,
        password_hash: String,
    }
    let maybe = sqlx::query_as::<_, LoginRow>(
        "SELECT id, username, created_at, password_hash FROM users WHERE username = $1",
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
        created_at: row.created_at,
    };
    let token = make_token(user.id, &state.jwt_secret)?;
    Ok(HttpResponse::Ok().json(AuthResp { token, user }))
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

fn verify_password(pw: &str, hash: &str) -> anyhow::Result<bool> {
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(pw.as_bytes(), &parsed)
        .is_ok())
}
