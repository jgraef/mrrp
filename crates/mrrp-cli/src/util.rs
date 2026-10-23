use std::{
    borrow::Cow,
    pin::Pin,
    str::FromStr,
    sync::OnceLock,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use chrono::{
    DateTime,
    TimeDelta,
    Utc,
};
use mrrp_core::signal::{
    AsyncReadSamples,
    GetSampleRate,
    ReadBuf,
    Remaining,
    SizeHint,
    StreamLength,
};
use num_traits::float::FloatCore;
use pin_project_lite::pin_project;
use serde::{
    Deserialize,
    Deserializer,
};
use tokio_util::sync::{
    CancellationToken,
    WaitForCancellationFutureOwned,
};

pub fn parse_degrees_as_radians<T>(s: &str) -> Result<T, T::Err>
where
    T: FloatCore + FromStr,
{
    Ok(s.parse::<T>()?.to_radians())
}

pub fn shutdown_signal() -> CancellationToken {
    static ONCE: OnceLock<CancellationToken> = OnceLock::new();

    ONCE.get_or_init(|| {
        let cancellation_token = CancellationToken::new();

        // todo: sigterm, etc.

        tokio::spawn({
            let cancellation_token = cancellation_token.clone();
            async move {
                if let Err(error) = tokio::signal::ctrl_c().await {
                    tracing::error!(%error, "Ctrl-C signal returned an error");
                }

                tracing::info!("Received Ctrl-C. Shutting down.");
                cancellation_token.cancel();
            }
        });

        cancellation_token
    })
    .clone()
}

pub async fn run_til_shutdown<R>(fut: impl Future<Output = R>) -> Option<R> {
    let cancellation_token = shutdown_signal();
    tokio::select! {
        _ = cancellation_token.cancelled() => None,
        output = fut => Some(output),
    }
}

pub fn deserialize_human_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
    humantime::parse_duration(&s).map_err(serde::de::Error::custom)
}

pub fn deserialize_human_time_delta<'de, D>(deserializer: D) -> Result<TimeDelta, D::Error>
where
    D: Deserializer<'de>,
{
    let duration = deserialize_human_duration(deserializer)?;
    TimeDelta::from_std(duration).map_err(serde::de::Error::custom)
}

pub async fn sleep_until(time: DateTime<Utc>) {
    let sleep_interval = TimeDelta::minutes(1);

    loop {
        let duration = time - Utc::now();
        if duration > TimeDelta::zero() {
            let sleep_duration = duration.min(sleep_interval);
            tokio::time::sleep(sleep_duration.to_std().unwrap()).await;
        }
        else {
            break;
        }
    }
}

pin_project! {
    pub struct CancelStream<R> {
        #[pin]
        inner: R,
        #[pin]
        cancelled: WaitForCancellationFutureOwned,
        token: CancellationToken,
    }
}

impl<R> CancelStream<R> {
    #[inline]
    pub fn new(inner: R) -> Self {
        let token = CancellationToken::new();
        Self::from_token(inner, token)
    }

    #[inline]
    pub fn from_token(inner: R, token: CancellationToken) -> Self {
        let cancelled = token.clone().cancelled_owned();
        Self {
            inner,
            cancelled,
            token,
        }
    }

    #[inline]
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    #[inline]
    pub fn cancel(&self) {
        self.token.cancel();
    }
}

impl<R> AsyncReadSamples for CancelStream<R>
where
    R: AsyncReadSamples,
{
    type Sample = R::Sample;
    type Error = R::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        if let Poll::Ready(()) = this.cancelled.poll(cx) {
            Poll::Ready(Ok(()))
        }
        else {
            this.inner.poll_read_samples(cx, buffer)
        }
    }
}

impl<R> StreamLength for CancelStream<R>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        Remaining::Unknown
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        SizeHint {
            lower_bound: 0,
            upper_bound: self.inner.size_hint().upper_bound,
        }
    }
}

impl<R> GetSampleRate for CancelStream<R>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}
