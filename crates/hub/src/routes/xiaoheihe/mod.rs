//! 小黑盒（xiaoheihe.cn）相关路由。
//!
//! - /xiaoheihe/news                     游戏新闻流。
//! - /xiaoheihe/discount/:platform       各平台游戏折扣。
//! - /xiaoheihe/add2cart/:platform       各平台喜加一。
//! - /xiaoheihe/user/:id                 用户动态。

pub mod add2cart;
pub mod discount;
pub mod news;
pub mod user;
pub mod util;
