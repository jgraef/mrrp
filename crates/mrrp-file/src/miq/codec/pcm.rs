use std::{
    io::{
        Read,
        Write,
    },
    marker::PhantomData,
};

use mrrp_core::sample::encoding::{
    Bytes,
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
    Encoder(E),
}

#[derive(Debug, Default)]
pub struct Pcm<E> {
    _marker: PhantomData<fn(&E)>,
}

impl<T, E> Encoder<T> for Pcm<E>
where
    T: Encode<E>,
    E: Endianess,
{
    type Error = Error<T::Error>;

    fn write_samples<W>(&mut self, samples: &[T], output: &mut W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        // todo: this might have room for optimization. handling each sample
        // individually might be slow, but this would really need some
        // investigation (look at codegen, benchmark)

        for sample in samples {
            let bytes = sample.encode().map_err(Error::Encoder)?;
            output.write_all(bytes.as_slice())?;
        }
        Ok(())
    }
}

impl<T, E> Decoder<T> for Pcm<E> {
    type Error = !;

    fn read_samples<R>(&mut self, buffer: &mut [T], data: &[u8]) -> Result<usize, Self::Error>
    where
        R: Read,
    {
        todo!();
    }
}
