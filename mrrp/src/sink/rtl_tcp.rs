use num_complex::Complex;
use rtlsdr_async::{
    DongleInfo,
    TunerType,
    rtl_tcp::{
        Command,
        server::ConnectionHandler,
    },
};
use tokio::net::TcpStream;

use crate::{
    io::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
    },
    sample::{
        FromSample,
        IntoSample,
        Sample,
    },
    source::rtlsdr::convert_complex_to_iq,
};

pub type Error<H> = rtlsdr_async::rtl_tcp::server::Error<H>;

pub struct StreamHandler<R, S> {
    stream: R,
    dongle_info: DongleInfo,
    buffer: Vec<S>,
}

impl<R, S> StreamHandler<R, S> {
    pub fn new(stream: R) -> Self {
        let dongle_info = DongleInfo {
            tuner_type: TunerType::UNKNOWN,
            tuner_gain_count: 0,
        };
        Self {
            stream,
            dongle_info,
            buffer: vec![],
        }
    }
}

impl<R, S> ConnectionHandler for StreamHandler<R, S>
where
    R: AsyncReadSamples<S> + Send + Unpin,
    S: Sample + Send,
    Complex<u8>: FromSample<S>,
{
    type Error = R::Error;

    fn dongle_info(&self) -> DongleInfo {
        self.dongle_info
    }

    async fn handle_command(&mut self, _command: Command) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn read_samples(
        &mut self,
        buffer: &mut [rtlsdr_async::Iq],
    ) -> Result<usize, Self::Error> {
        self.buffer.resize_with(buffer.len(), || S::EQUILIBRIUM);
        let num_samples = self.stream.read_samples(&mut self.buffer).await?;
        for i in 0..num_samples {
            buffer[i] = convert_complex_to_iq(self.buffer[i].into_sample());
        }
        Ok(num_samples)
    }
}

pub async fn serve_connection<R>(connection: TcpStream, stream: R) -> Result<(), Error<R::Error>>
where
    R: AsyncReadSamples<Complex<f32>> + Send + Unpin,
{
    rtlsdr_async::rtl_tcp::server::serve_connection(
        connection,
        Default::default(),
        StreamHandler::new(stream),
    )
    .await
}
