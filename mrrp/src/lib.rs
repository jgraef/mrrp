#![feature(allocator_api)]
#![feature(get_mut_unchecked)]
#![feature(maybe_uninit_write_slice)]
#![feature(maybe_uninit_slice)]

#[cfg(feature = "audio")]
pub mod audio;
pub mod buf;
pub mod chunk;
pub mod filter;
pub mod io;
pub mod modem;
pub mod sample;
pub mod sink;
pub mod source;
pub mod util;
