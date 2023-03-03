#![doc = include_str!("../README.md")]
// #![warn(missing_docs, rustdoc::broken_intra_doc_links)]
#![cfg_attr(not(any(feature = "std", test)), no_std)]
mod atomic;
mod metric;
pub mod registry;
pub use self::metric::*;
