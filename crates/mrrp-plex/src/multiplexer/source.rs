use std::fmt::Debug;

use mrrp_core::{
    sample::Complex,
    signal::AsyncReadSamples,
};
use mrrp_util::signal::broadcast;
use tokio::sync::{
    oneshot,
    watch,
};

use crate::{
    multiplexer::{
        Error,
        Multiplexer,
        reactor::Command,
        sink::Sink,
    },
    protocol::{
        StreamId,
        StreamInfo,
    },
};

pub type SourceError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub trait Source:
    AsyncReadSamples<Sample = Complex<f32>, Error = SourceError> + Debug + Send + Sync + 'static
{
}

impl<T> Source for T where
    T: AsyncReadSamples<Sample = Complex<f32>, Error = SourceError> + Debug + Send + Sync + 'static
{
}

#[derive(Clone, Debug)]
pub struct StreamHandle {
    id: StreamId,
    //data_subscriber: broadcast::Subscriber<Complex<f32>>,
    info_receiver: watch::Receiver<StreamInfo>,
    multiplexer: Multiplexer,
}

impl StreamHandle {
    #[inline]
    pub(super) fn new(
        id: StreamId,
        info_receiver: watch::Receiver<StreamInfo>,
        multiplexer: Multiplexer,
    ) -> Self {
        Self {
            id,
            info_receiver,
            multiplexer,
        }
    }

    #[inline]
    pub fn id(&self) -> StreamId {
        self.id
    }

    #[inline]
    pub fn info(&self) -> StreamInfo {
        self.info_receiver.borrow().clone()
    }

    #[inline]
    pub fn info_tracked(&mut self) -> StreamInfo {
        self.info_receiver.borrow_and_update().clone()
    }

    #[inline]
    pub async fn wait_info(&mut self) -> Option<StreamInfo> {
        self.info_receiver.changed().await.ok()?;
        Some(self.info_tracked())
    }

    pub async fn forward<S>(&self, sink: S) -> Result<(), Error>
    where
        S: Sink,
    {
        let (response_sender, response_receiver) = oneshot::channel();

        self.multiplexer
            .send_command_and_wait_for_response(
                Command::Forward {
                    stream_id: self.id,
                    sink: Box::new(sink),
                    response_sender,
                },
                response_receiver,
            )
            .await
    }
}
