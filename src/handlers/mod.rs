use actix_web::web;

pub mod auth;
pub mod chats;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(auth::register)
       .service(auth::login)
       .service(chats::start_direct_chat)
       .service(chats::list_messages);
}
