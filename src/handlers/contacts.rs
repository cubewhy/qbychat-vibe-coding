use crate::auth::{internal_err, AuthUser};
use crate::models::{AddContactReq, Contact, ContactDto, User};
use crate::state::AppState;
use actix_web::{delete, get, post, web, HttpResponse};
use serde::Serialize;
use sqlx::types::Uuid;

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

fn error_response(code: &str, message: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(ErrorResponse {
        error: ErrorDetail {
            code: code.to_string(),
            message: message.to_string(),
        },
    })
}

#[get("/v1/api/contacts")]
pub async fn list_contacts(
    state: web::Data<AppState>,
    user: AuthUser,
) -> actix_web::Result<HttpResponse> {
    let contacts: Vec<Contact> = sqlx::query_as(
        "SELECT user_id, contact_user_id, status, added_at FROM contacts WHERE user_id = $1",
    )
    .bind(user.0)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_err)?;

    let mut dtos = Vec::new();
    for contact in contacts {
        let contact_user: User = sqlx::query_as(
            "SELECT id, username, bio, is_online, last_seen_at, online_status_visibility, created_at FROM users WHERE id = $1",
        )
        .bind(contact.contact_user_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_err)?;
        dtos.push(ContactDto {
            user: contact_user,
            status: contact.status,
            added_at: contact.added_at,
        });
    }

    Ok(HttpResponse::Ok().json(dtos))
}

#[post("/v1/api/contacts/add")]
pub async fn add_contact(
    state: web::Data<AppState>,
    user: AuthUser,
    req: web::Json<AddContactReq>,
) -> actix_web::Result<HttpResponse> {
    if req.user_id == user.0 {
        return Ok(error_response("INVALID_REQUEST", "Cannot add yourself as contact"));
    }

    // Check if user exists
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM users WHERE id = $1")
        .bind(req.user_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_err)?;
    if exists.is_none() {
        return Ok(HttpResponse::NotFound().json(ErrorResponse {
            error: ErrorDetail {
                code: "NOT_FOUND".to_string(),
                message: "User not found".to_string(),
            },
        }));
    }

    // Check if already contact
    let existing: Option<Contact> = sqlx::query_as(
        "SELECT user_id, contact_user_id, status, added_at FROM contacts WHERE user_id = $1 AND contact_user_id = $2",
    )
    .bind(user.0)
    .bind(req.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_err)?;

    if let Some(contact) = existing {
        if contact.status == "blocked" {
            return Ok(HttpResponse::Conflict().json(ErrorResponse {
                error: ErrorDetail {
                    code: "CONFLICT".to_string(),
                    message: "User is blocked".to_string(),
                },
            }));
        } else {
            return Ok(HttpResponse::Conflict().json(ErrorResponse {
                error: ErrorDetail {
                    code: "CONFLICT".to_string(),
                    message: "Already a contact".to_string(),
                },
            }));
        }
    }

    sqlx::query(
        "INSERT INTO contacts (user_id, contact_user_id, status) VALUES ($1, $2, 'friend')",
    )
    .bind(user.0)
    .bind(req.user_id)
    .execute(&state.pool)
    .await
    .map_err(internal_err)?;

    Ok(HttpResponse::Created().finish())
}

#[delete("/v1/api/contacts/{user_id}")]
pub async fn remove_contact(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> actix_web::Result<HttpResponse> {
    let contact_user_id = path.into_inner();

    let res = sqlx::query(
        "DELETE FROM contacts WHERE user_id = $1 AND contact_user_id = $2 AND status = 'friend'",
    )
    .bind(user.0)
    .bind(contact_user_id)
    .execute(&state.pool)
    .await
    .map_err(internal_err)?;

    if res.rows_affected() == 0 {
        return Ok(HttpResponse::NotFound().json(ErrorResponse {
            error: ErrorDetail {
                code: "NOT_FOUND".to_string(),
                message: "Not a contact".to_string(),
            },
        }));
    }

    Ok(HttpResponse::NoContent().finish())
}

#[post("/v1/api/contacts/{user_id}/block")]
pub async fn block_contact(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> actix_web::Result<HttpResponse> {
    let contact_user_id = path.into_inner();

    // Remove if friend, then add as blocked
    sqlx::query("DELETE FROM contacts WHERE user_id = $1 AND contact_user_id = $2")
        .bind(user.0)
        .bind(contact_user_id)
        .execute(&state.pool)
        .await
        .map_err(internal_err)?;

    sqlx::query(
        "INSERT INTO contacts (user_id, contact_user_id, status) VALUES ($1, $2, 'blocked')",
    )
    .bind(user.0)
    .bind(contact_user_id)
    .execute(&state.pool)
    .await
    .map_err(internal_err)?;

    Ok(HttpResponse::NoContent().finish())
}

#[delete("/v1/api/contacts/{user_id}/block")]
pub async fn unblock_contact(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> actix_web::Result<HttpResponse> {
    let contact_user_id = path.into_inner();

    let res = sqlx::query(
        "DELETE FROM contacts WHERE user_id = $1 AND contact_user_id = $2 AND status = 'blocked'",
    )
    .bind(user.0)
    .bind(contact_user_id)
    .execute(&state.pool)
    .await
    .map_err(internal_err)?;

    if res.rows_affected() == 0 {
        return Ok(HttpResponse::NotFound().json(ErrorResponse {
            error: ErrorDetail {
                code: "NOT_FOUND".to_string(),
                message: "Not blocked".to_string(),
            },
        }));
    }

    Ok(HttpResponse::NoContent().finish())
}