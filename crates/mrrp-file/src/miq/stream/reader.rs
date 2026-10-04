use std::io::Read;

use crate::miq::{
    container::{
        self,
        header::SubFormat,
    },
    stream::SUB_FORMAT,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Container(#[from] container::reader::Error),

    #[error("{0}")]
    Decoder(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Invalid sub format: {sub_format:?}")]
    InvalidSubFormat {
        sub_format: Option<SubFormat<String>>,
    },
}

impl Error {
    #[inline]
    pub fn from_decoder<E>(error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Decoder(Box::new(error))
    }
}

#[derive(Debug)]
pub struct Reader<R> {
    container_reader: container::reader::Reader<R>,
}

impl<R> Reader<R>
where
    R: Read,
{
    pub fn new(reader: R) -> Result<Self, Error> {
        let mut container_reader = container::reader::Reader::new(reader);

        // reader MIQ header
        let file_header = container_reader.read_file_header()?;

        // check subformat ID and version
        if let Some(sub_format) = file_header.sub_format {
            if sub_format.id != SUB_FORMAT.id || sub_format.version > SUB_FORMAT.version {
                return Err(Error::InvalidSubFormat {
                    sub_format: Some(sub_format),
                });
            }
        }
        else {
            return Err(Error::InvalidSubFormat { sub_format: None });
        }

        Ok(Self { container_reader })
    }
}
