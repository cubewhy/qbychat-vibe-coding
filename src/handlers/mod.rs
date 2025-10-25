use actix_web::web;

pub mod auth;
pub mod chats;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(auth::register)
       .service(auth::login)
       .service(chats::start_direct_chat)
       .service(chats::create_group)
       .service(chats::create_channel)
       .service(chats::add_participant)
       .service(chats::list_messages);
}
