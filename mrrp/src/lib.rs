#![feature(allocator_api)]
#![feature(get_mut_unchecked)]
#![feature(maybe_uninit_write_slice)]
#![feature(maybe_uninit_slice)]

pub mod buf;
pub mod chunk;
pub mod demod;
pub mod io;
pub mod sample;
