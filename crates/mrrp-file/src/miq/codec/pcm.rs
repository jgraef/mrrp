use std::{
    io::Write,
    marker::PhantomData,
};

use mrrp_core::sample::encoding::{
    BigEndian,
    Bytes,
    Decode,
    Encode,
    Endianess,
};

use crate::miq::codec::{
    Decoder,
    Encoder,
};

#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Codec(E),
}

#[derive(Debug)]
pub struct Pcm<E = BigEndian> {
    _marker: PhantomData<fn(&E)>,
}

impl<E> Default for Pcm<E> {
    #[inline]
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T, E> Encoder<T> for Pcm<E>
where
    T: Encode<E>,
    E: Endianess,
{
    type Error = Error<T::Error>;

    fn write_samples<W>(&mut self, samples: &[T], mut output: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        // todo: this might have room for optimization. handling each sample
        // individually might be slow, but this would really need some
        // investigation (look at codegen, benchmark)

        for sample in samples {
            let bytes = sample.encode().map_err(Error::Codec)?;
            output.write_all(bytes.as_slice())?;
        }
        Ok(())
    }
}

impl<T, E> Decoder<T> for Pcm<E>
where
    T: Decode<E>,
    E: Endianess,
{
    type Error = Error<T::Error>;

    fn read_samples(&mut self, data: &[u8], buffer: &mut Vec<T>) -> Result<(), Self::Error> {
        for chunk in data.chunks_exact(T::Encoded::LEN) {
            let bytes = <T::Encoded as Bytes>::from_slice(chunk);
            buffer.push(T::decode(bytes).map_err(Error::Codec)?);
        }

        Ok(())
    }
}
