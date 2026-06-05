//! Client and server implementation for the `rtl_tcp` protocol.
//!
//! The protocol is outlined [here][1], but the `rtl_tcp` [source code] was used
//! for reference.
//!
//! [1]: https://k3xec.com/rtl-tcp/
//! [2]: https://github.com/rtlsdrblog/rtl-sdr-blog/blob/master/src/rtl_tcp.c

//pub mod client;
pub mod protocol;
pub mod server;

use std::fmt::Debug;

/// The type of tuner in a [`RtlSdr`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TunerType(pub u32);

impl TunerType {
    pub const UNKNOWN: Self = Self(0);
    pub const E4000: Self = Self(1);
    pub const FC0012: Self = Self(2);
    pub const FC0013: Self = Self(3);
    pub const FC2580: Self = Self(4);
    pub const R820T: Self = Self(5);
    pub const R828D: Self = Self(6);
}

impl TunerType {
    pub fn is_r82xx(&self) -> bool {
        matches!(*self, TunerType::R828D | TunerType::R820T)
    }
}

impl Debug for TunerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::UNKNOWN => write!(f, "TunerType::UNKNOWN"),
            Self::E4000 => write!(f, "TunerType::E4000"),
            Self::FC0012 => write!(f, "TunerType::FC0012"),
            Self::FC0013 => write!(f, "TunerType::FC0013"),
            Self::FC2580 => write!(f, "TunerType::FC2580"),
            Self::R820T => write!(f, "TunerType::R820T"),
            Self::R828D => write!(f, "TunerType::R828D"),
            _ => write!(f, "TunerType({})", self.0),
        }
    }
}

/// Information about the SDR dongle that is sent by the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DongleInfo {
    /// Tuner type as reported by librtlsdr
    pub tuner_type: TunerType,

    /// Number of gain levels supported by the tuner.
    pub tuner_gain_count: u32,
}
