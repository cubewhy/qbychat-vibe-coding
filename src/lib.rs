pub mod auth;
pub mod gif;
pub mod handlers;
pub mod models;
pub mod state;
pub mod upload;
pub mod ws;

use actix_web::dev::ServiceRequest;
use actix_web::{web, App};
use state::AppState;

pub fn app_factory(state: AppState) -> App<impl actix_web::dev::ServiceFactory<ServiceRequest>> {
    App::new()
        .app_data(web::Data::new(state.clone()))
        .configure(handlers::config)
        .service(ws::ws_route)
        .default_service(web::route().to(|| async { actix_web::HttpResponse::NotFound().finish() }))
}

use sqlx::PgPool;
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
