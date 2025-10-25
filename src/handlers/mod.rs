use actix_web::web;

pub mod auth;
pub mod chats;
pub mod users;
pub mod files;
pub mod uploads;
pub mod admin;
pub mod messages;
pub mod members;
pub mod chat_list;
pub mod members_notify;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(auth::register)
       .service(auth::login)
       .service(chats::start_direct_chat)
       .service(chats::create_group)
       .service(chats::create_channel)
       .service(chats::add_participant)
       .service(chats::promote_admin)
       .service(chats::demote_admin)
       .service(chats::remove_participant)
       .service(chats::mute_member)
       .service(chats::unmute_member)
       .service(chats::list_messages)
       .service(users::upload_avatars)
       .service(users::set_primary)
       .service(users::list_avatars)
       .service(files::create_download_token)
       .service(files::download_file)
       .service(uploads::upload_files)
       .service(admin::purge_storage)
       .service(messages::send_message)
       .service(messages::edit_message)
       .service(messages::delete_message)
       .service(messages::read_bulk)
       .service(messages::purge_reads)
       .service(messages::unread_count)
       .service(members::get_note)
       .service(members::set_note)
       .service(members::delete_note)
       .service(members_notify::get_notify)
       .service(members_notify::set_notify)
       .service(members_notify::get_mentions)
       .service(members_notify::clear_mentions)
       .service(chat_list::list_chats);
}
