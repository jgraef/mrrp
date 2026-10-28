use bytes::BufMut;
use mrrp_core::buf::SampleBuf;

pub mod pcm;

// todo: these traits definitely need some work. but we can figure out what
// works best once we write a encoder that produces packets.

pub trait Encoder<T> {
    type Error: std::error::Error + Send + Sync + 'static;

    fn write_samples<R, W>(&mut self, samples: R, buffer: W) -> Result<Flush, Self::Error>
    where
        R: SampleBuf<T>,
        W: BufMut;
}

pub trait Decoder<T> {
    type Error: std::error::Error + Send + Sync + 'static;

    fn read_samples(&mut self, data: &[u8], buffer: &mut Vec<T>) -> Result<(), Self::Error>;
}

/// Used by [`Encoder::write_samples`] to declare when the output buffer should
/// be flushed.
///
/// This is used by encoders to ensure the chunks in the file correspond to the
/// chunking required by the encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flush {
    /// Do not flush yet.
    Buffer,

    /// Flush now.
    Flush,
}

impl Flush {
    #[inline]
    pub fn flush_if(condition: bool) -> Self {
        if condition {
            Flush::Flush
        }
        else {
            Flush::Buffer
        }
    }
}
