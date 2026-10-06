//! # mrrp IQ file format
//!
//! See [`container`] for the general structure of this file format. Ontop of
//! the container format we define rules for a file format used for storing and
//! streaming IQ data.

use crate::miq::container::header::{
    SubFormat,
    Version,
};

pub mod codec;
pub mod container;
pub mod stream;
