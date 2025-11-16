pub mod bangumi_media;
pub mod bangumi_season;
pub mod hot_search;
pub mod link_news;
pub mod popular;
pub mod ranking;
pub mod rules;
pub mod user_dynamic;
pub mod user_video;

use crate::hub::types::{RouteMeta, RouteRegistration};

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

pub const ROUTE_REGISTRATIONS: [RouteRegistration; 8] = [
    hot_search::ROUTE_BILIBILI_HOT_SEARCH,
    popular::ROUTE_BILIBILI_POPULAR,
    link_news::ROUTE_BILIBILI_LINK_NEWS,
    ranking::ROUTE_BILIBILI_RANKING,
    user_video::ROUTE_BILIBILI_USER_VIDEO,
    user_dynamic::ROUTE_BILIBILI_USER_DYNAMIC,
    bangumi_season::ROUTE_BILIBILI_BANGUMI_SEASON,
    bangumi_media::ROUTE_BILIBILI_BANGUMI_MEDIA,
];
