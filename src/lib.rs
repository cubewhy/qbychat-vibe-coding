pub mod state;
pub mod models;
pub mod auth;
pub mod handlers;
pub mod ws;
pub mod upload;

use actix_web::{App, web};
use actix_web::dev::ServiceRequest;
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
