use std::{
    io::Write as _,
    str::FromStr,
};

use tokio::{
    io::{
        AsyncBufReadExt,
        AsyncWriteExt,
        BufReader,
    },
    net::{
        TcpStream,
        ToSocketAddrs,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Response is not UTF-8")]
    InvalidResponseEncoding(#[from] std::str::Utf8Error),

    #[error("Response code invalid: {code}")]
    InvalidRprtCode { code: String },

    #[error("Server replied with error: {0}")]
    Rprt(RprtCode),

    #[error("Expected RPRT, but received: {response}")]
    ExpectedRprt { response: String },

    #[error(transparent)]
    InvalidInt(#[from] std::num::ParseIntError),
}

#[derive(Debug)]
pub struct RigCtlClient {
    stream: BufReader<TcpStream>,
    buffer: Vec<u8>,
}

impl RigCtlClient {
    pub async fn connect(address: impl ToSocketAddrs) -> Result<Self, Error> {
        let stream = BufReader::new(TcpStream::connect(address).await?);

        Ok(Self {
            stream,
            buffer: Vec::with_capacity(1024),
        })
    }

    async fn send_buffered_command(&mut self) -> Result<(), Error> {
        self.stream.write_all(&mut self.buffer).await?;
        self.stream.flush().await?;
        self.buffer.clear();
        Ok(())
    }

    async fn receive_response(&mut self) -> Result<Result<&'_ str, RprtCode>, Error> {
        self.stream.read_until(b'\n', &mut self.buffer).await?;

        let line = str::from_utf8(&self.buffer[..self.buffer.len() - 1])
            .map_err(Error::InvalidResponseEncoding)?;
        tracing::debug!(line, "response");

        if let Some(rprt_code) = line.strip_prefix("RPRT ") {
            let rprt_code: i32 = rprt_code.parse().map_err(|_| {
                Error::InvalidRprtCode {
                    code: rprt_code.to_owned(),
                }
            })?;
            Ok(Err(RprtCode(rprt_code)))
        }
        else {
            Ok(Ok(line))
        }
    }

    async fn receive_and_parse_response<T, E>(
        &mut self,
        parser: impl FnOnce(&str) -> Result<T, E>,
    ) -> Result<T, Error>
    where
        Error: From<E>,
    {
        let line = self.receive_response().await?.map_err(Error::Rprt)?;
        Ok(parser(line)?)
    }

    async fn receive_and_handle_rprt(&mut self) -> Result<(), Error> {
        match self.receive_response().await? {
            Ok(line) => {
                Err(Error::ExpectedRprt {
                    response: line.to_owned(),
                })
            }
            Err(rprt) => {
                if rprt.is_ok() {
                    Ok(())
                }
                else {
                    Err(Error::Rprt(rprt))
                }
            }
        }
    }

    pub async fn get_frequency(&mut self) -> Result<u64, Error> {
        writeln!(&mut self.buffer, "f").unwrap();
        self.send_buffered_command().await?;

        self.receive_and_parse_response(FromStr::from_str).await
    }

    pub async fn set_frequency(&mut self, frequency: u64) -> Result<(), Error> {
        writeln!(&mut self.buffer, "F {frequency}").unwrap();
        self.send_buffered_command().await?;

        self.receive_and_handle_rprt().await
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display)]
pub struct RprtCode(pub i32);

impl RprtCode {
    pub fn is_ok(&self) -> bool {
        // SDR++ being SDR++ doesn't follow spec and returns 1 on set_freq
        self.0 >= 0
    }
}
