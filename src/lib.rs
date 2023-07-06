#![doc = include_str!("../README.md")]
// #![warn(missing_docs, rustdoc::broken_intra_doc_links)]
#![cfg_attr(not(any(feature = "std", test)), no_std)]

mod metric;
pub mod registry;
#[cfg(feature = "timestamp")]
pub(crate) mod timestamp;
pub use self::metric::*;

#[cfg(feature = "timestamp")]
pub use self::timestamp::UnixTimestamp;
