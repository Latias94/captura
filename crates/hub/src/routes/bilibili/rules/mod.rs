pub mod bangumi;
pub mod dynamic;
pub mod hot_search;
pub mod link_news;
pub mod popular;
pub mod ranking;
pub mod user_video;
pub mod utils;

pub use bangumi::rule as bilibili_bangumi_season_rule;
pub use dynamic::fetch_user_dynamic as fetch_user_dynamic_dynamic;
pub use hot_search::rule as bilibili_hot_search_rule;
pub use link_news::rule as bilibili_link_news_rule;
pub use popular::rule as bilibili_popular_rule;
pub use ranking::rule as bilibili_ranking_rule;
pub use user_video::rule as bilibili_user_video_rule;
