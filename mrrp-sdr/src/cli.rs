use std::path::PathBuf;

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
    /// Runs the app.
    ///
    /// This is most likely what you want to use. All other commands are for
    /// niche and debugging usecases.
    Ui(UiCommand),

    /// List radios that are connected.
    ListRadios,
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
    pub center_frequency: Option<f32>,

    #[clap(short, long)]
    pub sample_rate: Option<f32>,

    #[clap(long)]
    pub reset_app_state: bool,

    #[clap(long)]
    pub dont_save_app_state: bool,

    #[clap(long)]
    pub file: Option<PathBuf>,
}
