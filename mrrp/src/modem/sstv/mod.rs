//! # References
//!
//! - <http://lionel.cordesses.free.fr/gpages/sstv.html>
//! - <https://web.archive.org/web/20120505141047/http://www.cs.helsinki.fi/u/okraisan/slowrx/>
//! - <http://www.barberdsp.com/downloads/Dayton%20Paper.pdf>
//! - <https://web.archive.org/web/20120313215600/http://lionel.cordesses.free.fr/gpages/Cordesses.pdf>

mod decoder;
mod encoder;
pub mod image;
pub mod modes;
pub mod state;

pub use decoder::{
    DecodeError,
    SstvDecoder,
};
pub use encoder::SstvEncoder;

pub const LEADER_TONE: f32 = 1900.0;
pub const LEADER_TIME: f32 = 0.300;

pub const LEADER_BREAK_TIME: f32 = 0.010;

pub const VIS_BIT_TIME: f32 = 0.030;
pub const VIS_LOW_TONE: f32 = 1300.0;
pub const VIS_HIGH_TONE: f32 = 1100.0;

// sync, leader break, vis start/stop
pub const SYNC_TONE: f32 = 1200.0;

pub const PORCH_TONE: f32 = 1500.0;

pub const CHANNEL_LOW_TONE: f32 = 1500.0;
pub const CHANNEL_HIGH_TONE: f32 = 2300.0;
