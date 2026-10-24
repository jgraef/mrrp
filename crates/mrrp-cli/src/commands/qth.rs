use anyhow::Error;
use clap::{
    Parser,
    Subcommand,
};
use mrrp_geo::{
    HorizontalGeodetic,
    Qth,
};

use crate::Context;

pub async fn run(args: Args, _context: Context) -> Result<(), Error> {
    match args.command {
        Command::Encode {
            longitude,
            latitude,
            extended,
        } => {
            let geodetic = HorizontalGeodetic {
                latitude,
                longitude,
            };
            let qth = Qth::from_geodetic(geodetic, extended);
            println!("{qth}");
        }
        Command::Decode { locator } => {
            let geodetic = locator.geodetic();
            println!("Latitude: {:.3}°", geodetic.latitude);
            println!("Longitude: {:.3}°", geodetic.longitude);
        }
    }

    Ok(())
}

/// QTH commands
#[derive(Clone, Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    Encode {
        latitude: f64,
        longitude: f64,
        #[clap(short = 'x', long)]
        extended: bool,
    },
    Decode {
        locator: Qth,
    },
}
