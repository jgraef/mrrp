pub mod pcm;

// todo: these traits definitely need some work. but we can figure out what
// works best once we write a encoder that produces packets.

pub trait Encoder<T> {
    type Error: std::error::Error + Send + Sync + 'static;

    fn write_samples(&mut self, samples: &[T], buffer: &mut Vec<u8>) -> Result<Flush, Self::Error>;
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

    /// Flush or don't.
    Any,
}
