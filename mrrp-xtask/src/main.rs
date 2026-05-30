use std::{
    fs::File,
    io::{
        BufWriter,
        Write,
        stdout,
    },
    path::PathBuf,
};

use anyhow::{
    Error,
    anyhow,
};
use clap::{
    Parser,
    Subcommand,
};
use colorgrad::Gradient;
use dotenvy::dotenv;
use mrrp_widgets::colormap::ColorMap;

fn main() -> Result<(), Error> {
    let _ = dotenv();
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Command::ColormapPreset {
            output,
            samples,
            code,
            gradient,
        } => {
            let mut output: Box<dyn Write> = if let Some(output) = output {
                Box::new(BufWriter::new(File::create(output)?))
            }
            else {
                Box::new(stdout())
            };

            for gradient in &gradient {
                let name = &gradient;
                let gradient = preset_by_name(&gradient)
                    .ok_or_else(|| anyhow!("No such preset gradient: {gradient}"))?;
                let mut colormap = ColorMap::from_colograd(samples, gradient);

                let format: Box<dyn FnOnce(&ColorMap, &mut Box<dyn Write>) -> Result<(), Error>> =
                    if code {
                        Box::new(|colormap: &ColorMap, mut writer: &mut Box<dyn Write>| {
                            write!(&mut writer, "const {}: &[Color] = &[", name.to_uppercase())?;
                            for color in colormap.lut() {
                                write!(&mut writer, "{color:?}, ",)?;
                            }
                            write!(&mut writer, "];\n")?;
                            Ok::<(), Error>(())
                        })
                    }
                    else {
                        Box::new(|colormap, writer| {
                            serde_json::to_writer_pretty(writer, &colormap).map_err(Error::from)
                        })
                    };

                format(&mut colormap, &mut output)?;
            }
        }
    }

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    ColormapPreset {
        #[clap(short, long)]
        output: Option<PathBuf>,

        #[clap(short, long, default_value = "256")]
        samples: usize,

        #[clap(short, long)]
        code: bool,

        gradient: Vec<String>,
    },
}

fn preset_by_name(name: &str) -> Option<Box<dyn Gradient>> {
    match name {
        "blues" => Some(colorgrad::preset::blues().boxed()),
        "br_bg" => Some(colorgrad::preset::br_bg().boxed()),
        "bu_gn" => Some(colorgrad::preset::bu_gn().boxed()),
        "bu_pu" => Some(colorgrad::preset::bu_pu().boxed()),
        "cividis" => Some(colorgrad::preset::cividis().boxed()),
        "cool" => Some(colorgrad::preset::cool().boxed()),
        "cubehelix_default" => Some(colorgrad::preset::cubehelix_default().boxed()),
        "gn_bu" => Some(colorgrad::preset::gn_bu().boxed()),
        "greens" => Some(colorgrad::preset::greens().boxed()),
        "greys" => Some(colorgrad::preset::greys().boxed()),
        "inferno" => Some(colorgrad::preset::inferno().boxed()),
        "magma" => Some(colorgrad::preset::magma().boxed()),
        "or_rd" => Some(colorgrad::preset::or_rd().boxed()),
        "oranges" => Some(colorgrad::preset::oranges().boxed()),
        "pi_yg" => Some(colorgrad::preset::pi_yg().boxed()),
        "plasma" => Some(colorgrad::preset::plasma().boxed()),
        "pr_gn" => Some(colorgrad::preset::pr_gn().boxed()),
        "pu_bu" => Some(colorgrad::preset::pu_bu().boxed()),
        "pu_bu_gn" => Some(colorgrad::preset::pu_bu_gn().boxed()),
        "pu_or" => Some(colorgrad::preset::pu_or().boxed()),
        "pu_rd" => Some(colorgrad::preset::pu_rd().boxed()),
        "purples" => Some(colorgrad::preset::purples().boxed()),
        "rainbow" => Some(colorgrad::preset::rainbow().boxed()),
        "rd_bu" => Some(colorgrad::preset::rd_bu().boxed()),
        "rd_gy" => Some(colorgrad::preset::rd_gy().boxed()),
        "rd_pu" => Some(colorgrad::preset::rd_pu().boxed()),
        "rd_yl_bu" => Some(colorgrad::preset::rd_yl_bu().boxed()),
        "rd_yl_gn" => Some(colorgrad::preset::rd_yl_gn().boxed()),
        "reds" => Some(colorgrad::preset::reds().boxed()),
        "sinebow" => Some(colorgrad::preset::sinebow().boxed()),
        "spectral" => Some(colorgrad::preset::spectral().boxed()),
        "turbo" => Some(colorgrad::preset::turbo().boxed()),
        "viridis" => Some(colorgrad::preset::viridis().boxed()),
        "warm" => Some(colorgrad::preset::warm().boxed()),
        "yl_gn" => Some(colorgrad::preset::yl_gn().boxed()),
        "yl_gn_bu" => Some(colorgrad::preset::yl_gn_bu().boxed()),
        "yl_or_br" => Some(colorgrad::preset::yl_or_br().boxed()),
        "yl_or_rd" => Some(colorgrad::preset::yl_or_rd().boxed()),
        _ => None,
    }
}
