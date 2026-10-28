use std::marker::PhantomData;

use mrrp_core::{
    buf::SampleBuf,
    sample::encoding::{
        BigEndian,
        Bytes,
        Decode,
        Encode,
        Endianess,
    },
};

use crate::codec::{
    Decoder,
    Encoder,
    Flush,
};

#[derive(Clone, Copy, Debug)]
pub struct EncoderOptions {
    pub samples_per_chunk: usize,
}

impl Default for EncoderOptions {
    #[inline]
    fn default() -> Self {
        Self {
            samples_per_chunk: 1024,
        }
    }
}

#[derive(Debug)]
pub struct PcmEncoder<E = BigEndian> {
    current_chunk_sample_count: usize,
    options: EncoderOptions,
    _marker: PhantomData<fn(&E)>,
}

impl<E> PcmEncoder<E> {
    #[inline]
    pub fn new(options: EncoderOptions) -> Self {
        Self {
            current_chunk_sample_count: 0,
            options,
            _marker: PhantomData,
        }
    }
}

impl<E> Default for PcmEncoder<E> {
    #[inline]
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T, E> Encoder<T> for PcmEncoder<E>
where
    T: Encode<E>,
    E: Endianess,
{
    type Error = !;

    fn write_samples<R, W>(&mut self, samples: R, mut buffer: W) -> Result<Flush, Self::Error>
    where
        R: mrrp_core::buf::SampleBuf<T>,
        W: bytes::BufMut,
    {
        let remaining_samples_in_this_chunk =
            self.options.samples_per_chunk - self.current_chunk_sample_count;
        let mut samples = samples.take(remaining_samples_in_this_chunk);

        while samples.has_remaining() {
            //let remaining = samples.remaining();

            let samples_slice = samples.chunk();

            //tracing::debug!(?remaining, chunk_len = ?samples_slice.len(),
            // limited = remaining_samples_in_this_chunk);

            assert!(!samples_slice.is_empty());

            for sample in samples_slice {
                buffer.put_slice(sample.encode().as_slice());
            }

            let n = samples_slice.len();
            self.current_chunk_sample_count += n;
            samples.advance(n);
        }

        if self.current_chunk_sample_count == self.options.samples_per_chunk {
            self.current_chunk_sample_count = 0;
            Ok(Flush::Flush)
        }
        else {
            Ok(Flush::Buffer)
        }
    }
}

#[derive(Debug)]
pub struct PcmDecoder<E = BigEndian> {
    _marker: PhantomData<fn(&E)>,
}

impl<T, E> Decoder<T> for PcmDecoder<E>
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
