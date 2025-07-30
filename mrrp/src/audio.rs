// should we make audio sinks? currently this just turns any stream into rodio
// sources.

use std::{
    sync::{
        Arc,
        atomic::{
            AtomicBool,
            Ordering,
        },
    },
    task::{
        Context,
        Poll,
        Wake,
        Waker,
    },
    time::Duration,
};

use futures_util::FutureExt;
use parking_lot::Mutex;
use rodio::Source as _;
use tokio::{
    runtime::Handle,
    sync::oneshot,
};

use crate::io::{
    AsyncReadSamples,
    AsyncReadSamplesExt,
    EofError,
    GetSampleRate,
    Remaining,
};

#[derive(Debug)]
#[non_exhaustive]
pub struct RodioSource<R>
where
    R: AsyncReadSamples<f32>,
{
    /// the actual stream that we'll poll for samples
    read_samples: R,

    /// a sender to send `Ok(())` or any errors back.
    result_sender: Option<oneshot::Sender<Result<(), R::Error>>>,

    /// This flag is set when the inner stream returns `Poll::Pending`. This is
    /// to prevent constantly polling the stream everytime rodio asks us for a
    /// new sample. This flag implements `Wake`, and will be reset when woken.
    is_pending: Arc<IsPending>,

    /// A waker derived from `is_pending`. This can be turned into a [`Context`]
    /// and used for polling the inner stream.
    waker: Waker,

    /// If the inner stream uses the tokio runtime, it would usually panic,
    /// because it's being run in a separate thread managed by rodio.
    /// Therefore we get a handle to the tokio runtime when the [`RodioSource`]
    /// is created and pass it with it (in this field). Then, when we poll the
    /// inner stream, we enter the runtime.
    runtime_handle: Option<Handle>,

    total_duration: Option<Duration>,
}

impl<R> RodioSource<R>
where
    R: AsyncReadSamples<f32> + GetSampleRate,
{
    pub fn new(read_samples: R) -> Self {
        let is_pending = Arc::new(IsPending::default());
        let waker = Waker::from(is_pending.clone());
        let runtime_handle = Handle::try_current().ok();

        let total_duration = match read_samples.remaining() {
            Remaining::Finite { num_samples } => {
                Some(Duration::from_secs_f32(
                    num_samples as f32 / read_samples.sample_rate(),
                ))
            }
            Remaining::Infinite | Remaining::Unknown => None,
        };

        Self {
            read_samples,
            result_sender: None,
            is_pending,
            waker,
            runtime_handle,
            total_duration,
        }
    }

    pub fn with_result_sender(
        mut self,
        result_sender: oneshot::Sender<Result<(), R::Error>>,
    ) -> Self {
        self.result_sender = Some(result_sender);
        self
    }
}

impl<R> rodio::Source for RodioSource<R>
where
    R: AsyncReadSamples<f32> + GetSampleRate + Unpin,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        match self.read_samples.remaining() {
            Remaining::Finite { num_samples } => Some(num_samples),
            Remaining::Infinite | Remaining::Unknown => None,
        }
    }

    #[inline]
    fn channels(&self) -> rodio::ChannelCount {
        1
    }

    #[inline]
    fn sample_rate(&self) -> rodio::SampleRate {
        self.read_samples.sample_rate() as u32
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }
}

impl<R> Iterator for RodioSource<R>
where
    R: AsyncReadSamples<f32> + Unpin,
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_pending.is_pending() {
            // the inner stream was pending and we set a flag indicating this. this flag
            // will be reset by the waker passed to the poll method.

            // return silence
            Some(0.0)
        }
        else {
            // enter tokio runtime, if we have a handle
            let _runtime_guard = self
                .runtime_handle
                .as_ref()
                .map(|runtime_handle| runtime_handle.enter());

            // construct context from waker that we made
            let mut cx = Context::from_waker(&self.waker);

            // poll read_sample future to get one sample
            match self.read_samples.read_sample().poll_unpin(&mut cx) {
                Poll::Pending => {
                    // stream is pending. remember this so we don't constantly poll the stream
                    self.is_pending.set_pending();

                    // return silence
                    Some(0.0)
                }
                Poll::Ready(Err(EofError::Other(error))) => {
                    // send error to any result receiver to inidicate that the stream finished with
                    // an error
                    if let Some(result_sender) = self.result_sender.take() {
                        let _ = result_sender.send(Err(error));
                    }

                    // end rodio source
                    None
                }
                Poll::Ready(Err(EofError::Eof { .. })) => {
                    // read_sample returned eof, so the stream finished.

                    // send Ok(()) to any result receiver to indicate the stream finished without
                    // errors
                    if let Some(result_sender) = self.result_sender.take() {
                        let _ = result_sender.send(Ok(()));
                    }

                    // end rodio source
                    None
                }
                Poll::Ready(Ok(sample)) => {
                    // return sample
                    Some(sample)
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("audio error")]
pub enum Error<S> {
    Playback(#[from] rodio::StreamError),
    Stream(S),
    Dropped,
}

pub async fn play_audio<S>(signal: S, volume: f32) -> Result<(), Error<S::Error>>
where
    S: AsyncReadSamples<f32> + GetSampleRate + Unpin + Send + 'static,
    S::Error: Send,
{
    let (result_sender, done_receiver) = oneshot::channel();

    let signal = signal.buffered(0x4000);
    //let signal = signal.throttle_to_sample_rate();
    let source = RodioSource::new(signal).with_result_sender(result_sender);

    global_output_stream()?.mixer().add(
        source
            .automatic_gain_control(1.0, 4.0, 0.0, 5.0)
            .amplify_normalized(volume),
    );

    match done_receiver.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            tracing::warn!(?error);
            Err(Error::Stream(error))
        }
        Err(_) => {
            // todo: investigate why rodio seems to drop the source just before the stream
            // is exhausted
            Ok(())
            //tracing::warn!("rodio stream dropped");
            //Err(Error::Dropped)
        }
    }
}

fn global_output_stream() -> Result<&'static rodio::OutputStream, rodio::StreamError> {
    static OUTPUT_STREAM: Mutex<Option<&'static rodio::OutputStream>> = Mutex::new(None);

    let mut output_stream = OUTPUT_STREAM.lock();

    if output_stream.is_none() {
        *output_stream = Some(Box::leak(Box::new(
            rodio::OutputStreamBuilder::open_default_stream()?,
        )));
    }

    Ok(output_stream.unwrap())
}

/// Wrapper around an [`AtomicBool`] that can be turned into a waker (if put
/// into an [`Arc`]).
#[derive(Debug, Default)]
struct IsPending {
    flag: AtomicBool,
}

impl Wake for IsPending {
    fn wake(self: Arc<Self>) {
        self.flag.store(false, Ordering::Relaxed);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.flag.store(false, Ordering::Relaxed);
    }
}

impl IsPending {
    pub fn is_pending(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    pub fn set_pending(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }
}
