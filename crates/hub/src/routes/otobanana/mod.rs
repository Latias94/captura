//! OTOBANANA related routes.
//!
//! Currently implemented:
//! - /otobanana/user/:id/cast        User cast posts (podcast-friendly).
//! - /otobanana/user/:id/livestream  User livestreams.
//! - /otobanana/user/:id             User timeline (cast + messages).

pub mod cast;
pub mod livestream;
pub mod timeline;
