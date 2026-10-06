pub mod pcm;

// todo: this whole thing is very generic and useful outside of miq. in fact we
// already have something very similar in `mrrp_util::signal::pcm`.

// todo: we really don't want to use std::io::Write here. its contract is a
// little bit inconvenient, and we know that we're writing into a Vec<u8> or
// some other kind of buffer. either way it must accept all bytes, as we don't
// want to handle half-written samples here (and should not have to).

// todo: should these use `SampleBuf`, `SampleBufMut` or `ReadBuf`?

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
