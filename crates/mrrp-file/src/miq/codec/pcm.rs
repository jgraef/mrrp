use std::marker::PhantomData;

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
    Flush,
};

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
    // this is required because we need to put the error into `miq::stream::writer::Error`
    T::Error: std::error::Error + Send + Sync + 'static,
{
    type Error = T::Error;

    fn write_samples(&mut self, samples: &[T], buffer: &mut Vec<u8>) -> Result<Flush, Self::Error> {
        // todo: this might have room for optimization. handling each sample
        // individually might be slow, but this would really need some
        // investigation (look at codegen, benchmark)

        for sample in samples {
            let bytes = sample.encode()?;
            buffer.extend_from_slice(bytes.as_slice());
        }

        // We don't care when the StreamWriter flushes
        Ok(Flush::Any)
    }
}

impl<T, E> Decoder<T> for Pcm<E>
where
    T: Decode<E>,
    E: Endianess,
    // this is required because we need to put the error into `miq::stream::reader::Error`
    T::Error: std::error::Error + Send + Sync + 'static,
{
    type Error = T::Error;

    fn read_samples(&mut self, data: &[u8], buffer: &mut Vec<T>) -> Result<(), Self::Error> {
        for chunk in data.chunks_exact(T::Encoded::LEN) {
            let bytes = <T::Encoded as Bytes>::from_slice(chunk);
            buffer.push(T::decode(bytes)?);
        }

        Ok(())
    }
}
