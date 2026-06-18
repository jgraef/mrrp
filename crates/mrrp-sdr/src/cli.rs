use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct Args {
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
