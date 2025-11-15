pub mod bangumi_media;
pub mod bangumi_season;
pub mod hot_search;
pub mod link_news;
pub mod popular;
pub mod ranking;
pub mod user_dynamic;
pub mod user_video;

use crate::types::RouteMeta;

pub const ROUTES: &[&RouteMeta] = &[
    &hot_search::META_BILIBILI_HOT_SEARCH,
    &ranking::META_BILIBILI_RANKING,
    &popular::META_BILIBILI_POPULAR,
    &link_news::META_BILIBILI_LINK_NEWS,
    &user_video::META_BILIBILI_USER_VIDEO,
    &user_dynamic::META_BILIBILI_USER_DYNAMIC,
    &bangumi_season::META_BILIBILI_BANGUMI_SEASON,
    &bangumi_media::META_BILIBILI_BANGUMI_MEDIA,
];
