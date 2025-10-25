use actix_web::web;

pub mod admin;
pub mod auth;
pub mod chat_list;
pub mod chats;
pub mod files;
pub mod gifs;
pub mod members;
pub mod members_notify;
pub mod messages;
pub mod pin;
pub mod stickers;
pub mod uploads;
pub mod users;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(auth::register)
        .service(auth::login)
        .service(chats::start_direct_chat)
        .service(chats::create_group)
        .service(chats::create_channel)
        .service(chats::add_participant)
        .service(chats::list_admins)
        .service(chats::promote_admin)
        .service(chats::demote_admin)
        .service(chats::remove_participant)
        .service(chats::mute_member)
        .service(chats::unmute_member)
        .service(chats::leave_chat)
        .service(chats::clear_messages)
        .service(chats::set_visibility)
        .service(chats::public_search)
        .service(chats::public_join)
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
        .service(messages::forward_messages)
        .service(messages::read_bulk)
        .service(messages::list_message_reads)
        .service(messages::purge_reads)
        .service(messages::unread_count)
        .service(members::get_note)
        .service(members::set_note)
        .service(members::delete_note)
        .service(members_notify::get_notify)
        .service(members_notify::set_notify)
        .service(members_notify::get_mentions)
        .service(members_notify::clear_mentions)
        .service(chat_list::list_chats)
        .service(pin::pin_message)
        .service(pin::unpin_message)
        .service(stickers::create_pack)
        .service(stickers::add_sticker)
        .service(stickers::install_pack)
        .service(stickers::uninstall_pack)
        .service(stickers::list_my_packs)
        .service(stickers::send_sticker)
        .service(gifs::search_gifs)
        .service(gifs::send_gif);
}
