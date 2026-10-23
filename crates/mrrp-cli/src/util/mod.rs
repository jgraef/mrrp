pub mod sdrpp;
pub mod si;
pub mod stream;

use std::{
    borrow::Cow,
    str::FromStr,
    sync::OnceLock,
    time::Duration,
};

use chrono::{
    DateTime,
    TimeDelta,
    Utc,
};
use num_traits::float::FloatCore;
use serde::{
    Deserialize,
    Deserializer,
};
use tokio_util::sync::CancellationToken;

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
    // note: we sleep at most a minute at a time and then recheck if the time
    // has come. this is to make edge-cases somewhat work, where the computer's
    // time changes, e.g. when the user changes it.
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
