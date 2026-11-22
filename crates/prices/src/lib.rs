//! The price list: one overview of a league's prices, downloaded in one
//! request, kept in one local file with its age, and searched by name with
//! no network.

pub mod fetch;
pub mod list;

pub use fetch::{FetchError, SOURCE, fetch};
pub use list::{FileError, ParseError, Price, PriceList, parse, sparkline};
