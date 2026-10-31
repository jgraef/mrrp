use std::{
    fmt::Display,
    fs::File,
    io::{
        BufRead,
        BufReader,
    },
    num::NonZero,
    path::{
        Path,
        PathBuf,
    },
    process::Stdio,
    sync::Arc,
    thread::available_parallelism,
};

use anyhow::{
    Error,
    bail,
};
use clap::{
    Parser,
    Subcommand,
};
use futures_util::future::try_join_all;
use md5::{
    Digest,
    Md5,
};
use mrrp_geo::Qth;
use tokio::io::AsyncBufReadExt;
use tracing::Instrument;

use crate::{
    commands::Context,
    util::sdrpp::{
        self,
        FileInfo,
    },
};

pub async fn run(args: Args, context: Context) -> Result<(), Error> {
    match args.command {
        Command::BatchCompress {
            parallelism,
            path,
            mut comment,
            comment_file,
            clobber,
            no_qth,
        } => {
            let parallelism = parallelism
                .and_then(|value| Ok(NonZero::new(value)).transpose())
                .unwrap_or_else(|| available_parallelism())?;
            tracing::debug!(?parallelism);

            if let Some(comment_file) = &comment_file {
                let file = BufReader::new(File::open(comment_file)?);
                for result in file.lines() {
                    let line = result?;
                    comment.push(line);
                }
            }

            let qth = if !no_qth && let Ok(location) = context.get_geolocation() {
                Some(Qth::from_geodetic(location.as_horizontal(), true))
            }
            else {
                None
            };

            batch_compress(&path, parallelism, comment, clobber, qth).await?
        }
    }

    Ok(())
}

#[derive(Clone, Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    /// Batch process a folder of 16bit WAV as recorded by SDR++
    ///
    /// This extracts information from the file names, if the default template
    /// in SDR++ was used. Additional metadata tags can be added via individual
    /// commands or from a file. These are embedde as Vorbis comments into the
    /// output file.
    ///
    /// This also verifies that the conversion was lossless by converting back
    /// and comparing md5 hashes of both files.
    BatchCompress {
        #[clap(short = 'j', long)]
        parallelism: Option<usize>,

        #[clap(short, long)]
        comment: Vec<String>,

        #[clap(short, long)]
        comment_file: Option<PathBuf>,

        /// Overwrite output files that already exist.
        #[clap(long)]
        clobber: bool,

        /// Don't add metadata tag with station QTH from config
        #[clap(long)]
        no_qth: bool,

        path: PathBuf,
    },
}

async fn batch_compress(
    path: impl AsRef<Path>,
    parallelism: NonZero<usize>,
    comments: impl IntoIterator<Item = String>,
    clobber: bool,
    qth: Option<Qth>,
) -> Result<(), Error> {
    let (job_sender, job_receiver) = flume::bounded(parallelism.get());

    let comments = comments.into_iter().collect::<Arc<[String]>>();

    let mut workers = Vec::with_capacity(parallelism.get());
    for i in 0..parallelism.get() {
        let span = tracing::info_span!("worker", ?i);
        let join_handle = tokio::spawn(worker(job_receiver.clone()).instrument(span));
        workers.push(join_handle);
    }

    for result in std::fs::read_dir(&path)? {
        let dir_entry = result?;
        let input_path = dir_entry.path();
        if input_path
            .extension()
            .is_some_and(|extension| extension == "wav")
        {
            let input_info = sdrpp::parse_path(&input_path)?;
            let output_path = input_path.with_extension("flac");

            if output_path.exists() && !clobber {
                tracing::info!("Output file exists. Skipping");
                continue;
            }

            job_sender
                .send_async(Job {
                    input_path,
                    output_path,
                    input_info,
                    comments: comments.clone(),
                    clobber,
                    qth,
                })
                .await
                .unwrap();
        }
    }

    drop(job_sender);
    try_join_all(workers).await?;

    Ok(())
}

#[derive(Debug)]
struct Job {
    input_path: PathBuf,
    output_path: PathBuf,
    input_info: FileInfo,
    comments: Arc<[String]>,
    clobber: bool,
    qth: Option<Qth>,
}

impl Job {
    async fn run(&mut self) -> Result<(), Error> {
        tracing::info!(input = ?self.input_path.file_name(), output = ?self.output_path.file_name(), info = ?self.input_info, "Processing file");

        let input_md5sum = md5sum(&self.input_path).await?;

        self.convert_to_flac(input_md5sum).await?;

        let output_md5sum = self.convert_back_and_md5sum().await?;

        if input_md5sum != output_md5sum {
            tracing::error!(input = ?self.input_path.file_name(), ?input_md5sum, ?output_md5sum, "Conversion wasn't bit-exact!");
            bail!("File didn't roundtrip");
        }

        Ok(())
    }

    async fn convert_to_flac(&self, md5sum: [u8; 16]) -> Result<(), Error> {
        let mut command = tokio::process::Command::new("sox_ng");

        // global options
        command.arg("--multi-threaded").arg("-R");
        if self.clobber {
            command.arg("--clobber");
        }
        else {
            command.arg("--no-clobber");
        }

        // input
        command.arg("-t").arg("wav").arg(&self.input_path);

        // output
        for comment in &*self.comments {
            if !comment.is_empty() {
                command.arg("--add-comment").arg(comment);
            }
        }
        add_comment(
            &mut command,
            "CENTER_FREQUENCY",
            self.input_info.center_frequency,
        );
        add_comment(
            &mut command,
            "START_TIME",
            self.input_info.timestamp.to_utc().format("%+"),
        );
        add_comment(&mut command, "DATE", self.input_info.timestamp.format("%F"));
        add_comment(&mut command, "MODE", "Baseband IQ");
        add_comment(
            &mut command,
            "ENCODER",
            "mrrp (https://github.com/jgraef/mrrp)",
        );
        add_comment(&mut command, "MD5SUM", const_hex::display(&md5sum));
        if let Some(qth) = self.qth {
            add_comment(&mut command, "LOCATION", &qth);
        }

        command.arg("-t").arg("flac").arg(&self.output_path);

        // convert to flac
        let exit_status = command.spawn()?.wait().await?;
        if !exit_status.success() {
            bail!("sox-ng returned an error: {exit_status:?}");
        }

        Ok(())
    }

    async fn convert_back_and_md5sum(&self) -> Result<[u8; 16], Error> {
        let mut child = tokio::process::Command::new("sox_ng")
            .arg("--multi-threaded")
            .arg("--no-clobber")
            .arg("-t")
            .arg("flac")
            .arg(&self.output_path)
            .arg("-t")
            .arg("wav")
            .arg("-")
            .stdout(Stdio::piped())
            .spawn()?;

        // read output and hash it
        let mut stdout = tokio::io::BufReader::new(child.stdout.take().unwrap());
        let mut hasher = Md5::new();
        loop {
            let buffer = stdout.fill_buf().await?;
            if buffer.is_empty() {
                break;
            }
            hasher.update(buffer);
            let n = buffer.len();
            stdout.consume(n);
        }

        // wait for process to finish
        let exit_status = child.wait().await?;
        if !exit_status.success() {
            bail!("sox-ng returned an error: {exit_status:?}");
        }

        Ok(hasher.finalize().0)
    }
}

async fn worker(jobs: flume::Receiver<Job>) {
    while let Ok(mut job) = jobs.recv_async().await {
        if let Err(error) = job.run().await {
            tracing::error!(%error, output = ?job.output_path.file_name(), "Job failed. Deleting output file");
            std::fs::remove_file(&job.output_path).ok();
        }
    }

    tracing::debug!("worker done");
}

fn add_comment(command: &mut tokio::process::Command, key: &str, value: impl Display) {
    assert!(!key.contains('='));
    command.arg("--add-comment").arg(format!("{key}={value}"));
}

async fn md5sum(path: impl AsRef<Path>) -> Result<[u8; 16], Error> {
    let mut hasher = Md5::new();
    let mut reader = BufReader::new(File::open(path)?);

    tokio::task::spawn_blocking(move || {
        loop {
            let buffer = reader.fill_buf()?;

            if buffer.is_empty() {
                break;
            }

            hasher.update(&buffer);

            let n = buffer.len();
            reader.consume(n);
        }

        Ok(hasher.finalize().0)
    })
    .await
    .unwrap()
}
