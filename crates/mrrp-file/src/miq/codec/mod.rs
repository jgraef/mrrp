pub mod pcm;

use std::io::Write;

// todo: this whole thing is very generic and useful outside of miq. in fact we
// already have something very similar in `mrrp_util::signal::pcm`.

// todo: we really don't want to use std::io::Write here. its contract is a
// little bit inconvenient, and we know that we're writing into a Vec<u8> or
// some other kind of buffer. either way it must accept all bytes, as we don't
// want to handle half-written samples here (and should not have to).

// todo: should these use `SampleBuf`, `SampleBufMut` or `ReadBuf`?

pub trait Encoder<T> {
    type Error;

    fn write_samples<W>(&mut self, samples: &[T], output: W) -> Result<(), Self::Error>
    where
        W: Write;
}

pub trait Decoder<T> {
    type Error;

    fn read_samples(&mut self, data: &[u8], buffer: &mut Vec<T>) -> Result<(), Self::Error>;
}
