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
use clap::ValueEnum;
use colorgrad::Gradient;
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

#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum ColorGradient {
    Blues,
    BrBg,
    BuGn,
    BuPu,
    Cividis,
    Cool,
    CubehelixDefault,
    GnBu,
    Greens,
    Greys,
    #[default]
    Inferno,
    Magma,
    OrRd,
    Oranges,
    PiYg,
    Plasma,
    PrGn,
    PuBu,
    PuBuGn,
    PuOr,
    PuRd,
    Purples,
    Rainbow,
    RdBu,
    RdGy,
    RdPu,
    RdYlBu,
    RdYlGn,
    Reds,
    Sinebow,
    Spectral,
    Turbo,
    Viridis,
    Warm,
    YlGn,
    YlGnBu,
    YlOrBr,
    YlOrRd,
}

impl ColorGradient {
    pub fn boxed(&self) -> Box<dyn Gradient> {
        let gradient: Box<dyn Gradient> = match self {
            Self::Blues => Box::new(colorgrad::preset::blues()),
            Self::BrBg => Box::new(colorgrad::preset::br_bg()),
            Self::BuGn => Box::new(colorgrad::preset::bu_gn()),
            Self::BuPu => Box::new(colorgrad::preset::bu_pu()),
            Self::Cividis => Box::new(colorgrad::preset::cividis()),
            Self::Cool => Box::new(colorgrad::preset::cool()),
            Self::CubehelixDefault => Box::new(colorgrad::preset::cubehelix_default()),
            Self::GnBu => Box::new(colorgrad::preset::gn_bu()),
            Self::Greens => Box::new(colorgrad::preset::greens()),
            Self::Greys => Box::new(colorgrad::preset::greys()),
            Self::Inferno => Box::new(colorgrad::preset::inferno()),
            Self::Magma => Box::new(colorgrad::preset::magma()),
            Self::OrRd => Box::new(colorgrad::preset::or_rd()),
            Self::Oranges => Box::new(colorgrad::preset::oranges()),
            Self::PiYg => Box::new(colorgrad::preset::pi_yg()),
            Self::Plasma => Box::new(colorgrad::preset::plasma()),
            Self::PrGn => Box::new(colorgrad::preset::pr_gn()),
            Self::PuBu => Box::new(colorgrad::preset::pu_bu()),
            Self::PuBuGn => Box::new(colorgrad::preset::pu_bu_gn()),
            Self::PuOr => Box::new(colorgrad::preset::pu_or()),
            Self::PuRd => Box::new(colorgrad::preset::pu_rd()),
            Self::Purples => Box::new(colorgrad::preset::purples()),
            Self::Rainbow => Box::new(colorgrad::preset::rainbow()),
            Self::RdBu => Box::new(colorgrad::preset::rd_bu()),
            Self::RdGy => Box::new(colorgrad::preset::rd_gy()),
            Self::RdPu => Box::new(colorgrad::preset::rd_pu()),
            Self::RdYlBu => Box::new(colorgrad::preset::rd_yl_bu()),
            Self::RdYlGn => Box::new(colorgrad::preset::rd_yl_gn()),
            Self::Reds => Box::new(colorgrad::preset::reds()),
            Self::Sinebow => Box::new(colorgrad::preset::sinebow()),
            Self::Spectral => Box::new(colorgrad::preset::spectral()),
            Self::Turbo => Box::new(colorgrad::preset::turbo()),
            Self::Viridis => Box::new(colorgrad::preset::viridis()),
            Self::Warm => Box::new(colorgrad::preset::warm()),
            Self::YlGn => Box::new(colorgrad::preset::yl_gn()),
            Self::YlGnBu => Box::new(colorgrad::preset::yl_gn_bu()),
            Self::YlOrBr => Box::new(colorgrad::preset::yl_or_br()),
            Self::YlOrRd => Box::new(colorgrad::preset::yl_or_rd()),
        };
        gradient
    }
}
