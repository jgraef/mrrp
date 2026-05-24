use clap::{
    Args,
    Parser,
    Subcommand,
};

#[derive(Debug, Parser)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Ui(UiCommand),
    ListDevices,
}

impl Default for Command {
    fn default() -> Self {
        Self::Ui(Default::default())
    }
}

#[derive(Debug, Default, Args)]
pub struct UiCommand {
    #[clap(short, long)]
    pub radio: Option<String>,

    #[clap(short = 'f', long)]
    pub center_frequency: Option<u64>,

    #[clap(short, long)]
    pub sample_rate: Option<u64>,
}
