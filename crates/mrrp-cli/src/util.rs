use std::{
    str::FromStr,
    sync::OnceLock,
};

use num_traits::float::FloatCore;
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
