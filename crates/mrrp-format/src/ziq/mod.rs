pub mod reader;
pub mod writer;

use bytes::{
    Buf,
    BufMut,
};

pub const FILE_SIGNATURE: [u8; 4] = *b"ZIQ_";
pub const HEADER_SIZE: usize = 22;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub compression: Compression,
    pub bits_per_sample: u8,
    pub sample_rate: u64,
    pub annotation_length: u64,
}

impl Header {
    pub fn decode<B>(mut buffer: B) -> Result<Self, reader::Error>
    where
        B: Buf,
    {
        let mut signature = [0u8; 4];
        buffer.copy_to_slice(&mut signature);
        if signature != FILE_SIGNATURE {
            return Err(reader::Error::InvalidFileSignature { value: signature });
        }

        let compression = buffer.get_u8().into();
        let bits_per_sample = buffer.get_u8();
        let sample_rate = buffer.get_u64_le();
        let annotation_length = buffer.get_u64_le();

        Ok(Self {
            compression,
            bits_per_sample,
            sample_rate,
            annotation_length,
        })
    }

    pub fn encode<B>(&self, mut buffer: B)
    where
        B: BufMut,
    {
        buffer.put_slice(&FILE_SIGNATURE);
        buffer.put_u8(self.compression.into());
        buffer.put_u8(self.bits_per_sample);
        buffer.put_u64_le(self.sample_rate);
        buffer.put_u64_le(self.annotation_length);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Compression {
    None,
    Zstd,
    Unknown(u8),
}

impl From<u8> for Compression {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Zstd,
            _ => Self::Unknown(value),
        }
    }
}

impl From<Compression> for u8 {
    fn from(value: Compression) -> Self {
        match value {
            Compression::None => 0,
            Compression::Zstd => 1,
            Compression::Unknown(value) => value,
        }
    }
}

#[cfg(test)]
mod tests {

    use mrrp_util::signal::{
        AsyncReadSamplesExt,
        write::AsyncWriteSamplesExt,
    };
    use num_complex::Complex;
    use serde::{
        Deserialize,
        Serialize,
    };

    #[cfg(feature = "zstd")]
    use crate::ziq::writer::Zstd;
    use crate::ziq::{
        Compression,
        reader::Reader,
        writer::{
            Uncompressed,
            Writer,
        },
    };

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Annotations {
        center_frequency: u32,
        title: String,
        operator: String,
        sat_id: String,
    }

    impl Default for Annotations {
        fn default() -> Self {
            Self {
                center_frequency: 437_400_000,
                title: "ARISS SSTV series 33, Student Education".to_owned(),
                operator: "switch".to_owned(),
                sat_id: "XSKZ-5603-1870-9019-3066".to_owned(),
            }
        }
    }

    impl Annotations {
        pub fn expected_annotations_length(&self) -> u64 {
            serde_json::to_vec(self).unwrap().len() as u64
        }
    }

    #[tokio::test]
    async fn test_uncompressed_roundtrip() {
        let mut buffer = vec![];

        let annotations = Annotations::default();
        let expected_annotations_length = annotations.expected_annotations_length();

        {
            let mut writer = Writer::new(&mut buffer, Uncompressed, 2_400_000, &annotations)
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

        {
            let reader = Reader::new(&buffer[..]).await.unwrap();

            let header = reader.header();
            assert_eq!(header.compression, Compression::None);
            assert_eq!(header.bits_per_sample, 16);
            assert_eq!(header.sample_rate, 2_400_000);
            assert_eq!(header.annotation_length, expected_annotations_length);

            assert_eq!(reader.annotations::<Annotations>().unwrap(), annotations);

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
    }

    #[cfg(feature = "zstd")]
    #[tokio::test]
    async fn test_compressed_roundtrip() {
        let mut buffer = vec![];

        let annotations = Annotations::default();
        let expected_annotations_length = annotations.expected_annotations_length();

        {
            let mut writer = Writer::new(&mut buffer, Zstd::default(), 2_400_000, &annotations)
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

        {
            let reader = Reader::new(&buffer[..]).await.unwrap();

            let header = reader.header();
            assert_eq!(header.compression, Compression::Zstd);
            assert_eq!(header.bits_per_sample, 16);
            assert_eq!(header.sample_rate, 2_400_000);
            assert_eq!(header.annotation_length, expected_annotations_length);

            assert_eq!(reader.annotations::<Annotations>().unwrap(), annotations);

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
    }
}
