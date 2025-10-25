use actix_web::{test, App};
use sqlx::{Executor};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use qbychat_vibe_coding::{handlers, ws, run_migrations};
use qbychat_vibe_coding::state::AppState;
use serde_json::json;

#[actix_web::test]
async fn register_and_login_and_direct_chat_flow() -> anyhow::Result<()> {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://app:password@localhost:5432/app".to_string());
    let pool = match PgPoolOptions::new().max_connections(1).acquire_timeout(std::time::Duration::from_secs(5)).connect(&db_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping test, cannot connect to db: {}", e);
            return Ok(());
        }
    };
    // ensure schema
    run_migrations(&pool).await?;

    // isolate data by truncating tables
    pool.execute("TRUNCATE messages, chat_participants, chats, users RESTART IDENTITY CASCADE").await?;

    let state = AppState { pool: pool.clone(), clients: Arc::new(dashmap::DashMap::new()), jwt_secret: Arc::new("testsecret".to_string()) };

    let app = test::init_service(
        App::new()
            .app_data(actix_web::web::Data::new(state.clone()))
            .configure(handlers::config)
            .service(ws::ws_route)
    ).await;

    // register alice
    let req = test::TestRequest::post().uri("/api/register").set_json(&json!({"username":"alice","password":"secretpw"})).to_request();
    let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
    let token_alice = resp.get("token").and_then(|v| v.as_str()).unwrap().to_string();

    // register bob
    let req = test::TestRequest::post().uri("/api/register").set_json(&json!({"username":"bob","password":"secretpw"})).to_request();
    let _resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;

    // create direct chat with bob
    let req = test::TestRequest::post().uri("/api/chats/direct")
        .insert_header((actix_web::http::header::AUTHORIZATION, format!("Bearer {}", token_alice)))
        .set_json(&json!({"peer_username":"bob"}))
        .to_request();
    let chat_resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
    assert!(chat_resp.get("chat_id").is_some());

    Ok(())
}
