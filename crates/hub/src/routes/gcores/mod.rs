//! GCORES 机核网相关路由。
//!
//! 对应 RSSHub 的 gcores 路由集合：
//! - /gcores/news
//! - /gcores/articles
//! - /gcores/videos
//! - /gcores/topics/recommend, /gcores/topics/:id/recommend
//! - /gcores/tags/:id/:tab?
//! - /gcores/collections/:id/:tab?
//! - /gcores/categories/:id/:tab?
//! - /gcores/radios/preview

pub mod articles;
pub mod categories;
pub mod collections;
pub mod news;
pub mod program_previews;
pub mod tags;
pub mod topics;
pub mod util;
pub mod videos;
