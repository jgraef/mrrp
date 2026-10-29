use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

pub use async_compression::Level;
use async_compression::tokio::write::ZstdEncoder;
use bytes::{
    BufMut,
    BytesMut,
};
use mrrp_core::{
    sample::encoding::{
        Encode,
        Encoding,
        LittleEndian,
    },
    signal::AsyncWriteSamples,
};
use mrrp_util::signal::pcm::PcmWriter;
use num_complex::Complex;
use serde::Serialize;
use tokio::io::{
    AsyncWrite,
    AsyncWriteExt,
};

use crate::ziq::{
    HEADER_SIZE,
    Header,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub struct Writer<W, S, C>
where
    S: Encoding,
    C: Compression,
{
    writer: PcmWriter<C::Writer<W>, S, LittleEndian>,
}

impl<W, S, C> Writer<W, S, C>
where
    W: AsyncWrite + Unpin,
    S: Encoding + BitsPerSample,
    C: Compression,
{
    #[inline]
    pub async fn new<A>(
        mut writer: W,
        compression: C,
        sample_rate: u64,
        annotations: A,
    ) -> Result<Self, Error>
    where
        A: Serialize,
    {
        let mut header = Header {
            compression: C::HEADER_VALUE,
            bits_per_sample: S::BITS_PER_SAMPLE,
            sample_rate,
            annotation_length: 0,
        };

        let mut buffer = BytesMut::new();

        // write annotations into buffer
        buffer.resize(HEADER_SIZE, 0);
        serde_json::to_writer((&mut buffer).writer(), &annotations)?;

        // write header
        header.annotation_length = (buffer.len() - HEADER_SIZE).try_into().unwrap();
        header.encode(&mut buffer[0..]);

        // write buffer
        writer.write_all_buf(&mut buffer).await?;

        let writer = PcmWriter::new(compression.new(writer));
        Ok(Self { writer })
    }
}

impl<W, S, C> Debug for Writer<W, S, C>
where
    S: Encoding,
    C: Compression,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // todo: at least show what sample type and compression is used
        f.debug_struct("Writer").finish_non_exhaustive()
    }
}

impl<W, S, C> AsyncWriteSamples<S> for Writer<W, S, C>
where
    W: AsyncWrite,
    S: Encode<LittleEndian>,
    C: Compression,
    <C as Compression>::Writer<W>: AsyncWrite + Unpin,
{
    type Error = Error;

    #[inline]
    fn poll_write_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        Pin::new(&mut self.writer)
            .poll_write_samples(cx, buffer)
            .map_err(Into::into)
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.writer)
            .poll_flush(cx)
            .map_err(Into::into)
    }

    #[inline]
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.writer)
            .poll_close(cx)
            .map_err(Into::into)
    }
}

pub trait Compression: sealed::Sealed {
    type Writer<W>;

    const HEADER_VALUE: super::Compression;

    fn new<W>(&self, writer: W) -> Self::Writer<W>
    where
        W: AsyncWrite;
}

mod sealed {
    pub trait Sealed {}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Uncompressed;

impl Compression for Uncompressed {
    type Writer<W> = W;

    const HEADER_VALUE: super::Compression = super::Compression::None;

    #[inline]
    fn new<W>(&self, writer: W) -> Self::Writer<W>
    where
        W: AsyncWrite,
    {
        writer
    }
}

impl sealed::Sealed for Uncompressed {}

#[derive(Clone, Copy, Debug, Default)]
pub struct Zstd {
    level: Level,
}

impl Compression for Zstd {
    type Writer<W> = ZstdEncoder<W>;

    const HEADER_VALUE: super::Compression = super::Compression::Zstd;

    #[inline]
    fn new<W>(&self, writer: W) -> Self::Writer<W>
    where
        W: AsyncWrite,
    {
        ZstdEncoder::with_quality(writer, self.level)
    }
}

impl sealed::Sealed for Zstd {}

pub trait BitsPerSample: sealed::Sealed {
    const BITS_PER_SAMPLE: u8;
}

macro_rules! samples {
    {$($ty:ty = $bits:expr;)*} => {
        $(
            impl BitsPerSample for Complex<$ty> {
                const BITS_PER_SAMPLE: u8 = $bits;
            }
            impl sealed::Sealed for Complex<$ty> {}
        )*
    };
}

samples! {
    i8 = 8;
    i16 = 16;
    f32 = 32;
}

#[cfg(test)]
mod tests {
    use bytes::Buf;
    use mrrp_util::signal::write::AsyncWriteSamplesExt;
    use num_complex::Complex;

    use crate::ziq::{
        Compression,
        HEADER_SIZE,
        Header,
        writer::{
            Uncompressed,
            Writer,
            Zstd,
        },
    };

    #[tokio::test]
    async fn write_uncompressed_file() {
        let mut buffer = vec![];

        {
            let mut writer = Writer::new(&mut buffer, Uncompressed, 2_400_000, ())
                .await
                .unwrap();

            // write 512 complex samples 0+1i, 2+3i, ...
            for i in 0i16..512 {
                writer
                    .write_sample(Complex {
                        re: i * 2,
                        im: i * 2 + 1,
                    })
                    .await
                    .unwrap();
            }

            writer.close().await.unwrap();
        }

        let header = Header::decode(&buffer[..HEADER_SIZE]).unwrap();

        assert_eq!(header.compression, Compression::None);
        assert_eq!(header.bits_per_sample, 16);
        assert_eq!(header.sample_rate, 2_400_000);
        assert_eq!(header.annotation_length, 4);

        assert_eq!(
            &buffer[HEADER_SIZE..][..header.annotation_length as usize],
            b"null"
        );

        let mut samples = &buffer[(HEADER_SIZE + header.annotation_length as usize)..];
        let mut i = 0;

        while samples.has_remaining() {
            assert_eq!(samples.get_i16_le(), i);
            assert_eq!(samples.get_i16_le(), i + 1);
            i += 2;
        }

        assert_eq!(i, 1024);
    }

    #[tokio::test]
    async fn write_compressed_file() {
        let mut buffer = vec![];

        {
            let mut writer = Writer::new(&mut buffer, Zstd::default(), 2_400_000, ())
                .await
                .unwrap();

            // write 512 complex samples 0+1i, 2+3i, ...
            for i in 0i16..512 {
                writer
                    .write_sample(Complex {
                        re: i * 2,
                        im: i * 2 + 1,
                    })
                    .await
                    .unwrap();
            }

            writer.close().await.unwrap();
        }

        let header = Header::decode(&buffer[..HEADER_SIZE]).unwrap();

        assert_eq!(header.compression, Compression::Zstd);
        assert_eq!(header.bits_per_sample, 16);
        assert_eq!(header.sample_rate, 2_400_000);
        assert_eq!(header.annotation_length, 4);

        assert_eq!(
            &buffer[HEADER_SIZE..][..header.annotation_length as usize],
            b"null"
        );

        let samples =
            zstd::decode_all(&buffer[HEADER_SIZE + header.annotation_length as usize..]).unwrap();
        let mut samples = &samples[..];
        let mut i = 0;

        while samples.has_remaining() {
            assert_eq!(samples.get_i16_le(), i);
            assert_eq!(samples.get_i16_le(), i + 1);
            i += 2;
        }

        assert_eq!(i, 1024);
    }
}
