//! # mrrp IQ file format
//!
//! See [`container`] for the general structure of this file format. Ontop of
//! the container format we define rules for a file format used for storing and
//! streaming IQ data.

pub mod codec;
pub mod container;
pub mod stream;
