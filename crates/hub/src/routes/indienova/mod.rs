//! indienova related routes.
//!
//! - /indienova/article           Indie game news / development.
//! - /indienova/column/:columnId  Column / topic.
//! - /indienova/usergames         User games library.
//! - /indienova/gamedb/recent     GameDB recent releases.

pub mod article;
pub mod column;
pub mod gamedb;
pub mod usergames;
pub mod util;
