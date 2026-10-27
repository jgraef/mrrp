use std::marker::PhantomData;

use mrrp_core::sample::encoding::{
    BigEndian,
    Bytes,
    Decode,
    Encode,
    Endianess,
};

use crate::codec::{
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
{
    type Error = !;

    fn write_samples(&mut self, samples: &[T], buffer: &mut Vec<u8>) -> Result<Flush, Self::Error> {
        // todo: this might have room for optimization. handling each sample
        // individually might be slow, but this would really need some
        // investigation (look at codegen, benchmark)

        for sample in samples {
            buffer.extend_from_slice(sample.encode().as_slice());
        }

        // We don't care when the StreamWriter flushes
        Ok(Flush::Any)
    }
}

impl<T, E> Decoder<T> for Pcm<E>
where
    T: Decode<E>,
    E: Endianess,
{
    type Error = !;

    fn read_samples(&mut self, data: &[u8], buffer: &mut Vec<T>) -> Result<(), Self::Error> {
        for chunk in data.chunks_exact(T::Encoded::LEN) {
            buffer.push(T::decode(&<T::Encoded as Bytes>::from_slice(chunk)));
        }

        Ok(())
    }
}
