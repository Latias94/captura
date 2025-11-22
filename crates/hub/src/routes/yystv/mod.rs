//! Yystv (游研社, yystv.cn) related routes.
//!
//! - /yystv/docs                 All articles.
//! - /yystv/category/:category   Category articles.
//! - /yystv/video/:category?     Video programs (optional sub-category).

pub mod category;
pub mod docs;
pub mod util;
pub mod video;
