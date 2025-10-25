use crate::state::AppState;
use actix_web::{self, http::header::HeaderMap, web};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header as JwtHeader, Validation};
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub fn make_token(user_id: Uuid, secret: &str) -> actix_web::Result<String> {
    let exp = (Utc::now() + chrono::Duration::days(30)).timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        exp,
    };
    jsonwebtoken::encode(
        &JwtHeader::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(internal_err)
}

pub fn decode_token(token: &str, secret: &str) -> Result<Uuid, jsonwebtoken::errors::Error> {
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )?;
    Ok(Uuid::parse_str(&data.claims.sub).expect("valid uuid in token"))
}

pub fn extract_bearer(headers: &HeaderMap) -> Result<String, ()> {
    let Some(value) = headers.get(actix_web::http::header::AUTHORIZATION) else {
        return Err(());
    };
    let Ok(s) = value.to_str() else {
        return Err(());
    };
    if let Some(rest) = s.strip_prefix("Bearer ") {
        Ok(rest.to_string())
    } else {
        Err(())
    }
}

#[derive(Clone)]
pub struct AuthUser(pub Uuid);

impl actix_web::FromRequest for AuthUser {
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let headers = req.headers().clone();
        let query = req.uri().query().map(|s| s.to_string());
        let secret = req
            .app_data::<web::Data<AppState>>()
            .unwrap()
            .jwt_secret
            .clone();
        Box::pin(async move {
            let token = if let Some(q) = query {
                if let Ok(qmap) =
                    web::Query::<std::collections::HashMap<String, String>>::from_query(&q)
                {
                    qmap.get("token").cloned()
                } else {
                    None
                }
            } else {
                None
            };
            let token = token.or_else(|| crate::auth::extract_bearer(&headers).ok());
            let token =
                token.ok_or_else(|| actix_web::error::ErrorUnauthorized("missing token"))?;
            let uid = crate::auth::decode_token(&token, &secret)
                .map_err(|_| actix_web::error::ErrorUnauthorized("invalid token"))?;
            Ok(AuthUser(uid))
        })
    }
}

pub fn internal_err<E: std::fmt::Debug>(e: E) -> actix_web::Error {
    actix_web::error::ErrorInternalServerError(format!("{:?}", e))
}
pub fn conflict_or_internal<E: std::fmt::Debug>(e: E) -> actix_web::Error {
    eprintln!("err: {:?}", e);
    actix_web::error::ErrorConflict(format!("{:?}", e))
}
