pub mod commands;
pub mod config;
pub mod files;
pub mod util;

use anyhow::{
    Error,
    anyhow,
};
use clap::{
    Parser,
    Subcommand,
    ValueEnum,
};
use mrrp_geo::Geodetic;
use mrrp_sat::{
    satellite::SatelliteDatabase,
    update::Updater,
};
use tabled::{
    grid::config::ColoredConfig,
    settings::{
        Style,
        TableOption,
    },
};
use tracing_subscriber::EnvFilter;

use crate::{
    config::Config,
    files::Files,
    util::partially_obscured,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let files = Files::open()?;
    let args = Args::parse();
    let config = files.config()?;

    run(args, files, config).await?;

    Ok(())
}

#[derive(Clone, Debug, Parser)]
struct Args {
    #[clap(flatten)]
    global: GlobalOptions,

    #[clap(subcommand)]
    command: Command,
}

macro_rules! subcommands {
    {$($variant:ident => $mod:ident,)*} => {
        #[derive(Clone, Debug, Subcommand)]
        enum Command {
            $(
                $variant(commands::$mod::Args),
            )*
        }

        async fn run(args: Args, files: Files, config: Config) -> Result<(), Error> {
            let Args { global, command } = args;
            match command {
                $(
                    Command::$variant(args) => {
                        commands::$mod::run(args, Context {
                            global,
                            files,
                            config,
                        })
                        .await?;
                    }
                )*
            }

            Ok(())
        }
    };
}

subcommands! {
    RtlSdr => rtl_sdr,
    Modem => modem,
    Sat => sat,
    Test => test,
    Qth => qth,
}

#[derive(Clone, Debug)]
pub struct Context {
    pub global: GlobalOptions,
    pub files: Files,
    pub config: Config,
}

impl Context {
    pub async fn open_satellite_db_with_updater(
        &self,
        suppress_auto_update: bool,
    ) -> Result<(SatelliteDatabase, Updater), Error> {
        let data_dir = self.files.data_dir();

        // open satellite data
        let mut satellites = SatelliteDatabase::open(data_dir.join("satellites.json"))?;
        if let Some(api_token) = &self.config.sat.satnogs_api_token {
            tracing::debug!(
                "SatNOGS api token set: {}",
                partially_obscured(api_token, 32, 'x')
            );
            satellites.set_satnogs_api_token(api_token.clone());
        }

        // perform update if necessary, but skip this invoked with update
        // command
        let updater = Updater::new(&data_dir, self.config.sat.min_update_interval);
        if !suppress_auto_update {
            updater.perform_auto_update(&mut satellites).await?;
        }

        Ok((satellites, updater))
    }

    #[inline]
    pub async fn open_satellite_db(&self) -> Result<SatelliteDatabase, Error> {
        let (satellites, _updater) = self.open_satellite_db_with_updater(false).await?;
        Ok(satellites)
    }

    pub fn get_geolocation(&self) -> Result<Geodetic, Error> {
        let location = *self
            .config
            .sat
            .location
            .as_ref()
            .ok_or_else(|| anyhow!("Station not configured"))?;

        Ok(location.to_geodetic())
    }
}

#[derive(Clone, Debug, clap::Args)]
pub struct GlobalOptions {
    /// How to format tables that are printed by any subcommands.
    #[clap(short, long, env = "MRRP_TABLE_STYLE", default_value = "sharp")]
    pub table_style: TableStyle,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum TableStyle {
    Empty,
    Blank,
    Ascii,
    Psql,
    Markdown,
    Modern,
    Sharp,
    Rounded,
    ModernRounded,
    Extended,
    Dots,
    ReStructuredText,
    AsciiRounded,
}

impl<Data, Dims> TableOption<Data, ColoredConfig, Dims> for TableStyle {
    fn change(self, records: &mut Data, cfg: &mut ColoredConfig, dimension: &mut Dims) {
        match self {
            TableStyle::Empty => Style::empty().change(records, cfg, dimension),
            TableStyle::Blank => Style::blank().change(records, cfg, dimension),
            TableStyle::Ascii => Style::ascii().change(records, cfg, dimension),
            TableStyle::Psql => Style::psql().change(records, cfg, dimension),
            TableStyle::Markdown => Style::markdown().change(records, cfg, dimension),
            TableStyle::Modern => Style::modern().change(records, cfg, dimension),
            TableStyle::Sharp => Style::sharp().change(records, cfg, dimension),
            TableStyle::Rounded => Style::rounded().change(records, cfg, dimension),
            TableStyle::ModernRounded => Style::modern_rounded().change(records, cfg, dimension),
            TableStyle::Extended => Style::extended().change(records, cfg, dimension),
            TableStyle::Dots => Style::dots().change(records, cfg, dimension),
            TableStyle::ReStructuredText => {
                Style::re_structured_text().change(records, cfg, dimension)
            }
            TableStyle::AsciiRounded => Style::ascii_rounded().change(records, cfg, dimension),
        }
    }
}
