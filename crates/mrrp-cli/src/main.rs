pub mod commands;
pub mod config;
pub mod files;
pub mod util;

use anyhow::Error;
use clap::{
    Parser,
    Subcommand,
    ValueEnum,
};
use tabled::{
    grid::config::ColoredConfig,
    settings::{
        Style,
        TableOption,
    },
};

use crate::{
    config::Config,
    files::Files,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt::init();

    let files = Files::open()?;
    let Args { global, command } = Args::parse();
    let config = files.config()?;

    match command {
        Command::RtlSdr(args) => {
            commands::rtl_sdr::run(Context {
                args,
                global,
                files,
                config,
            })
            .await?
        }
        Command::Sat(args) => {
            commands::sat::run(Context {
                args,
                global,
                files,
                config,
            })
            .await?
        }
    }

    Ok(())
}

pub struct Context<A> {
    pub args: A,
    pub global: GlobalOptions,
    pub files: Files,
    pub config: Config,
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(flatten)]
    global: GlobalOptions,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    RtlSdr(commands::rtl_sdr::Args),
    Sat(commands::sat::Args),
}

#[derive(Clone, Debug, clap::Args)]
pub struct GlobalOptions {
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
