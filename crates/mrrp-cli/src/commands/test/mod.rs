pub mod ariss;
pub mod miq;

use std::{
    fs::File,
    io::{
        BufReader,
        Read,
        Seek,
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
use image::{
    RgbaImage,
    imageops::FilterType,
};
use mrrp_audio::WavSource;
use mrrp_core::{
    sample::Complex,
    signal::{
        AsyncReadSamples,
        GetSampleRate,
    },
};
use mrrp_file::riff::{
    self,
    ChunkId,
};
use mrrp_util::signal::{
    AsyncReadSamplesExt,
    EofError,
};
use rustfft::FftPlanner;

use crate::{
    Context,
    util::ColorGradient,
};

pub async fn run(args: Args, context: Context) -> Result<(), Error> {
    match args.command {
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
        Command::Ariss(args) => ariss::run(args, context).await?,
        Command::RenderWaterfall {
            width,
            height,
            color,
            output,
            input,
        } => {
            let source = WavSource::<_, f32>::from_path(&input)?;

            let image = render_waterfall(source, width, height, color.boxed()).await?;

            tracing::info!(?output, "writing image");
            image.save(&output)?;
        }
        Command::Miq(args) => miq::run(args, context).await?,
    }

    Ok(())
}

/// Test commands
#[derive(Clone, Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    /// Print chunks contained in a RIFF file.
    ReadRiff {
        path: PathBuf,
    },
    Ariss(ariss::Args),
    RenderWaterfall {
        #[clap(short, long)]
        width: u32,

        #[clap(short, long)]
        height: u32,

        #[clap(short, long, default_value = "inferno")]
        color: ColorGradient,

        #[clap(short, long)]
        output: PathBuf,

        input: PathBuf,
    },
    Miq(miq::Args),
}

async fn render_waterfall<R>(
    mut input: R,
    width: u32,
    height: u32,
    gradient: impl Gradient,
) -> Result<RgbaImage, Error>
where
    R: AsyncReadSamples<Sample = f32> + GetSampleRate + Unpin,
    Error: From<R::Error>,
{
    let fft_size = (2 * height)
        .try_into()
        .map_err(|_| anyhow!("Image height overflow: {height}"))?;
    assert!(fft_size % 2 == 0);
    let fft_output_size = fft_size / 2;

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    let mut fft_scratch = vec![Complex::ZERO; fft.get_inplace_scratch_len()];
    let mut sample_buffer = vec![0.0; fft_size];
    let mut fft_buffer = vec![Complex::ZERO; fft_size];

    let mut pixel_buffer = vec![];
    let mut pixel_min = f32::INFINITY;
    let mut pixel_max = f32::NEG_INFINITY;

    // process input into a pixel buffer
    loop {
        match input.read_samples_exact(&mut sample_buffer).await {
            Ok(()) => {}
            Err(EofError::Eof { num_samples_read }) => {
                if num_samples_read == 0 {
                    // if there's 0 samples left, quit the loop
                    break;
                }

                // if the buffer was partially filled, fill the remainder of
                // buffer with zeros.
                sample_buffer[num_samples_read..].fill(0.0);
            }
            Err(EofError::Other(error)) => return Err(error.into()),
        }

        // convert to complex
        for i in 0..fft_size {
            fft_buffer[i].re = sample_buffer[i];
            fft_buffer[i].im = 0.0;
        }

        // process
        fft.process_with_scratch(&mut fft_buffer, &mut fft_scratch);

        // push magnitudes into pixel buffer
        for value in &fft_buffer[..fft_output_size] {
            // note: normally this would be scaled by 10 or 20 for a correct dB
            // scale, but we normalize afterwards anyway.
            let value = value.norm_sqr().log10();

            // track min/max for normalization
            if value < pixel_min {
                pixel_min = value
            };
            if value > pixel_max {
                pixel_max = value
            };

            pixel_buffer.push(value);
        }

        assert!(
            pixel_buffer.len() < 100_000_000,
            "pixel buffer too large ({}). TODO: resample pixel buffer",
            humansize::SizeFormatter::new(
                pixel_buffer.len() * 4,
                humansize::FormatSizeOptions::default()
            )
        );
    }

    // create image
    //
    // this image will not have the correct width yet. we could of course
    // resample our data to the desired width, but we can also let the image
    // crate to that for us after the image is created.
    assert!(pixel_buffer.len() % fft_output_size == 0);
    let width_raw = pixel_buffer.len() / fft_output_size;
    tracing::debug!(
        pixel_buffer_len = pixel_buffer.len(),
        width_raw,
        height,
        pixel_max,
        pixel_min,
        "creating image"
    );
    let mut image = RgbaImage::new(
        width_raw
            .try_into()
            .map_err(|_| anyhow!("Image width overflow: {width_raw}"))?,
        height,
    );

    for (x, col) in pixel_buffer.chunks_exact(fft_output_size).enumerate() {
        for (y, value) in col.into_iter().enumerate() {
            // normalize power to arbitrary scale
            let value = (value - pixel_min) / (pixel_max - pixel_min);

            // convert scalar to color
            let pixel = image::Rgba(gradient.at(value).to_rgba8());

            // note: the conversions for x and y can't fail, because width and
            // height have been converted to u32 before and didn't fail
            let x = x.try_into().unwrap();
            let y = height - 1 - u32::try_from(y).unwrap();
            image.put_pixel(x, y, pixel);
        }
    }

    // rescale image
    //
    // if output turns out funky (e.g. with transparency), check this functions
    // docs. it assumes things about the image that i'm not sure are met by the
    // output of colorgrad.
    let image = image::imageops::resize(&image, width, height, FilterType::Triangle);

    Ok(image)
}
