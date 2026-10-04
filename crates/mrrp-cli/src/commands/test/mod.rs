use std::{
    fs::File,
    io::{
        BufReader,
        Read,
        Seek,
    },
    path::PathBuf,
};

use anyhow::Error;
use clap::{
    Parser,
    Subcommand,
};
use mrrp_file::riff::{
    self,
    ChunkId,
};

use crate::Context;

pub async fn run(context: Context<Args>) -> Result<(), Error> {
    match context.args.command {
        Command::ReadRiff { path } => {
            fn print_chunk_info<R>(
                mut reader: &mut riff::Reader<R>,
                chunk_ref: riff::ChunkRef,
                level: usize,
            ) -> Result<(), Error>
            where
                R: Read + Seek,
            {
                for _ in 0..level {
                    print!(" ");
                }
                println!("- {chunk_ref:?}");

                if chunk_ref.id() == ChunkId::RIFF || chunk_ref.id() == ChunkId::LIST {
                    let mut children = vec![];
                    let mut list = reader.list(chunk_ref)?;
                    while let Some(child_ref) = list.next()? {
                        children.push(child_ref);
                    }

                    for child_ref in children {
                        print_chunk_info(&mut reader, child_ref, level)?;
                    }
                }

                Ok(())
            }

            let mut reader = riff::Reader::new(BufReader::new(File::open(&path)?));
            let chunk_ref = reader.riff()?;
            print_chunk_info(&mut reader, chunk_ref, 0)?;
        }
    }

    Ok(())
}

/// Test commands
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print chunks contained in a RIFF file.
    ReadRiff { path: PathBuf },
}
