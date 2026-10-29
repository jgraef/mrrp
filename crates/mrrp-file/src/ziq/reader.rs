use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

#[cfg(feature = "zstd")]
use async_compression::tokio::bufread::ZstdDecoder;
use bytes::{
    Bytes,
    BytesMut,
};
use mrrp_core::{
    sample::encoding::{
        Decode,
        LittleEndian,
    },
    signal::{
        AsyncReadSamples,
        GetSampleRate,
        ReadBuf,
        Remaining,
        SizeHint,
        StreamLength,
    },
};
use mrrp_util::signal::pcm::PcmReader;
use num_complex::Complex;
use serde::de::DeserializeOwned;
use tokio::io::{
    AsyncBufRead,
    AsyncRead,
    AsyncReadExt,
};

use crate::ziq::{
    Compression,
    Header,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Invalid file signature: {value:02x?}")]
    InvalidFileSignature { value: [u8; 4] },

    #[error("Unsupported compression: {value:?}")]
    UnsupportedCompression { value: Compression },

    #[error(
        "Expected {expected_bits_per_sample} bits per sample, but the file uses {file_bits_per_sample}."
    )]
    SampleFormatMismatch {
        expected_bits_per_sample: u8,
        file_bits_per_sample: u8,
    },

    #[error("The annotation size {size_in_file} is too large.")]
    AnnotationLengthOverflow { size_in_file: u64 },

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(derive_more::Debug)]
#[debug(bound())]
pub struct Reader<R, S>
where
    S: State,
{
    #[debug(skip)]
    reader: <S as State>::Reader<R>,
    header: Header,
    annotations: Bytes,
}

impl<R> Reader<R, Undetermined>
where
    R: AsyncBufRead + Unpin,
{
    pub async fn new(mut reader: R) -> Result<Self, Error> {
        let mut buffer = [0; 22];
        reader.read_exact(&mut buffer).await?;

        let header = Header::decode(buffer.as_slice())?;

        let annotations_size = header.annotation_length.try_into().map_err(|_| {
            Error::AnnotationLengthOverflow {
                size_in_file: header.annotation_length,
            }
        })?;
        let mut annotations = BytesMut::with_capacity(annotations_size);
        annotations.resize(annotations_size, 0);
        reader.read_exact(&mut annotations).await?;

        let reader = <Undetermined as State>::new(reader, &header)?;

        Ok(Self {
            reader,
            header,
            annotations: annotations.freeze(),
        })
    }
}

impl<R> Reader<R, Undetermined>
where
    R: AsyncBufRead,
{
    #[inline]
    pub fn with_sample_format<D>(self) -> Result<Reader<R, D>, Error>
    where
        D: State,
    {
        let reader = D::new(self.reader, &self.header)?;
        Ok(Reader {
            reader,
            header: self.header,
            annotations: self.annotations,
        })
    }
}

impl<R, S> Reader<R, S>
where
    S: State,
{
    #[inline]
    pub fn header(&self) -> &Header {
        &self.header
    }

    #[inline]
    pub fn annotations_bytes(&self) -> &Bytes {
        &self.annotations
    }

    #[inline]
    pub fn annotations<T>(&self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        Ok(serde_json::from_slice(&self.annotations)?)
    }
}

impl<R, S> AsyncReadSamples for Reader<R, S>
where
    R: AsyncRead,
    S: State,
    <S as State>::Reader<R>: AsyncReadSamples<Sample = S> + Unpin,
    Error: From<<<S as State>::Reader<R> as AsyncReadSamples>::Error>,
{
    type Sample = S;
    type Error = Error;

    #[inline]
    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.reader)
            .poll_read_samples(cx, buffer)
            .map_err(Into::into)
    }
}

impl<R, S> StreamLength for Reader<R, S>
where
    S: State,
    <S as State>::Reader<R>: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.reader.remaining()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        self.reader.size_hint()
    }
}

impl<R, S> GetSampleRate for Reader<R, S>
where
    S: State,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.header.sample_rate as f32
    }
}

pub trait State: sealed::Sealed {
    type Reader<R>;

    fn new<R>(reader: R, header: &Header) -> Result<Self::Reader<R>, Error>
    where
        R: AsyncBufRead;
}

pub enum Undetermined {}

impl State for Undetermined {
    type Reader<R> = R;

    #[inline]
    fn new<R>(reader: R, header: &Header) -> Result<Self::Reader<R>, Error>
    where
        R: AsyncBufRead,
    {
        let _ = header;
        Ok(reader)
    }
}

impl sealed::Sealed for Undetermined {}

pub enum CompressedReader<R, S>
where
    S: Decode<LittleEndian>,
{
    None(PcmReader<R, S, LittleEndian>),
    #[cfg(feature = "zstd")]
    Zstd(PcmReader<ZstdDecoder<R>, S, LittleEndian>),
}

impl<R, S> AsyncReadSamples for CompressedReader<R, S>
where
    R: AsyncBufRead + Unpin,
    S: Decode<LittleEndian>,
{
    type Sample = S;
    type Error = Error;

    #[inline]
    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        match &mut *self {
            CompressedReader::None(pcm_reader) => {
                Pin::new(pcm_reader)
                    .poll_read_samples(cx, buffer)
                    .map_err(Into::into)
            }
            CompressedReader::Zstd(pcm_reader) => {
                Pin::new(pcm_reader)
                    .poll_read_samples(cx, buffer)
                    .map_err(Into::into)
            }
        }
    }
}

impl<R, S> StreamLength for CompressedReader<R, S>
where
    S: Decode<LittleEndian>,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        match self {
            CompressedReader::None(pcm_reader) => pcm_reader.remaining(),
            CompressedReader::Zstd(pcm_reader) => pcm_reader.remaining(),
        }
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        match self {
            CompressedReader::None(pcm_reader) => pcm_reader.size_hint(),
            CompressedReader::Zstd(pcm_reader) => pcm_reader.size_hint(),
        }
    }
}

mod sealed {
    pub trait Sealed {}
}

macro_rules! samples {
    {$($ty:ty = $bits:expr;)*} => {
        $(
            impl State for Complex<$ty> {
                type Reader<R> = CompressedReader<R, Complex<$ty>>;

                #[inline]
                fn new<R>(reader: R, header: &Header) -> Result<Self::Reader<R>, Error>
                where
                    R: AsyncBufRead,
                {
                    if header.bits_per_sample == $bits {
                        match header.compression {
                            Compression::None => Ok(CompressedReader::None(PcmReader::new(reader))),
                            #[cfg(feature = "zstd")]
                            Compression::Zstd => Ok(CompressedReader::Zstd(PcmReader::new(ZstdDecoder::new(reader)))),
                            _ => Err(Error::UnsupportedCompression { value: header.compression }),
                        }
                    }
                    else {
                        Err(Error::SampleFormatMismatch {
                            expected_bits_per_sample: $bits,
                            file_bits_per_sample: header.bits_per_sample,
                        })
                    }
                }
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
    use byteorder::{
        LittleEndian,
        WriteBytesExt,
    };
    use bytes::{
        BufMut,
        BytesMut,
    };
    use mrrp_util::signal::AsyncReadSamplesExt;
    use num_complex::Complex;
    use serde::{
        Deserialize,
        Serialize,
    };

    use crate::ziq::{
        Compression,
        Header,
        reader::Reader,
    };

    #[tokio::test]
    async fn read_uncompressed_file() {
        let mut buffer = BytesMut::new();

        let header = Header {
            compression: Compression::None,
            bits_per_sample: 16,
            sample_rate: 2_400_000,
            annotation_length: 0,
        };
        header.encode(&mut buffer);

        // write 512 complex samples 0+1i, 2+3i, ...
        for i in 0..1024 {
            buffer.put_i16_le(i);
        }

        let reader = Reader::new(&*buffer).await.unwrap();
        assert_eq!(reader.header(), &header);

        let mut reader = reader.with_sample_format::<Complex<i16>>().unwrap();

        let mut i = 0;
        loop {
            let Some(sample) = reader.read_sample_or_eof().await.unwrap()
            else {
                break;
            };
            assert_eq!(sample.re, i);
            assert_eq!(sample.im, i + 1);
            i += 2;
        }

        assert_eq!(i, 1024);
    }

    #[tokio::test]
    async fn read_compressed_file() {
        let mut buffer = BytesMut::new();

        let header = Header {
            compression: Compression::Zstd,
            bits_per_sample: 16,
            sample_rate: 2_400_000,
            annotation_length: 0,
        };
        header.encode(&mut buffer);

        let mut zstd = zstd::Encoder::new((&mut buffer).writer(), 0).unwrap();

        // write 512 complex samples 0+1i, 2+3i, ...
        for i in 0..1024 {
            zstd.write_i16::<LittleEndian>(i).unwrap();
        }
        zstd.finish().unwrap();

        let reader = Reader::new(&*buffer).await.unwrap();
        assert_eq!(reader.header(), &header);

        let mut reader = reader.with_sample_format::<Complex<i16>>().unwrap();

        let mut i = 0;
        loop {
            let Some(sample) = reader.read_sample_or_eof().await.unwrap()
            else {
                break;
            };
            assert_eq!(sample.re, i);
            assert_eq!(sample.im, i + 1);
            i += 2;
        }

        assert_eq!(i, 1024);
    }

    #[tokio::test]
    async fn read_annotations_bytes() {
        let mut buffer = BytesMut::new();
        let annotations = b"Hello World";

        let header = Header {
            compression: Compression::None,
            bits_per_sample: 16,
            sample_rate: 2_400_000,
            annotation_length: annotations.len().try_into().unwrap(),
        };
        header.encode(&mut buffer);
        buffer.put_slice(annotations);

        let reader = Reader::new(&*buffer).await.unwrap();
        assert_eq!(reader.header(), &header);

        assert_eq!(&**reader.annotations_bytes(), &*annotations);
    }

    #[tokio::test]
    async fn read_annotations_json() {
        let mut buffer = BytesMut::new();

        #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct Annotations {
            center_frequency: u32,
            title: String,
            operator: String,
            sat_id: String,
        }
        let annotations = Annotations {
            center_frequency: 437_400_000,
            title: "ARISS SSTV series 33, Student Education".to_owned(),
            operator: "switch".to_owned(),
            sat_id: "XSKZ-5603-1870-9019-3066".to_owned(),
        };
        let annotations_json = serde_json::to_vec(&annotations).unwrap();

        let header = Header {
            compression: Compression::None,
            bits_per_sample: 16,
            sample_rate: 2_400_000,
            annotation_length: annotations_json.len().try_into().unwrap(),
        };
        header.encode(&mut buffer);
        buffer.put_slice(&annotations_json);

        let reader = Reader::new(&*buffer).await.unwrap();
        assert_eq!(reader.header(), &header);

        assert_eq!(reader.annotations::<Annotations>().unwrap(), annotations);
    }
}
