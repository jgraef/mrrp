// used in buf/samples_mut.rs
#![feature(get_mut_unchecked)]
// used in buf/uninit_slice.rs, but not strictly needed
#![feature(allocator_ext)]

pub mod buf;
pub mod chunk;
pub mod consts;
pub mod sample;
pub mod signal;
mod util;
