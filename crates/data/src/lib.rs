//! The data pack: what the application knows about the game, built from the
//! trade site's public lists and kept in one local file.

pub mod build;
pub mod classes;
pub mod fetch;
pub mod pack;

pub use build::{BuildError, build};
pub use classes::{CLASSES, category, is_local_class};
pub use fetch::{FetchError, fetch};
pub use pack::{Changes, Pack, PackError, changes};
