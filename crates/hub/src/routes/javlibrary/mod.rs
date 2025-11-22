//! JavLibrary related routes.
//!
//! Currently implemented:
//! - /javlibrary/newentries/:language?                   Latest entries list (simplified).
//! - /javlibrary/star/:id/:language?/:mode?              Videos list for a given star (simplified).
//! - /javlibrary/genre/:genre?/:language?/:mode?         Videos list for a given genre (simplified).
//! - /javlibrary/maker/:maker?/:language?/:mode?         Videos list for a given maker (simplified).
//! - /javlibrary/bestrated/:language?/:mode?             Best rated videos ranking (simplified).
//! - /javlibrary/mostwanted/:language?/:mode?            Most wanted videos ranking (simplified).
//! - /javlibrary/bestreviews/:language?/:mode?           Best reviews videos ranking (simplified).

pub mod bestrated;
pub mod bestreviews;
pub mod genre;
pub mod maker;
pub mod mostwanted;
pub mod newentries;
pub mod star;
