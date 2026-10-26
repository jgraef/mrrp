use std::{
    fs::File,
    io::BufReader,
    path::Path,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use anyhow::Error;
use crossterm::event::{
    Event,
    EventStream,
    KeyCode,
    KeyEvent,
    KeyEventKind,
};
use futures_util::{
    Stream,
    StreamExt,
    TryStreamExt,
};
use mrrp_audio::WavSource;
use mrrp_core::{
    sample::Complex,
    signal::{
        FiniteStream,
        GetSampleRate,
    },
};
use mrrp_rtl_tcp::{
    DongleInfo,
    TunerType,
    server::{
        Connection,
        Server,
    },
};
use mrrp_util::signal::{
    AsyncReadSamplesExt,
    Converted,
    write::AsyncWriteSamplesExt,
};
use ratatui::{
    Terminal,
    TerminalOptions,
    backend::CrosstermBackend,
};
use tokio::net::TcpListener;

use crate::util::run_til_shutdown;

pub async fn play(input: impl AsRef<Path>, listen_address: &str) -> Result<(), Error> {
    tracing::info!(path = ?input.as_ref(), "Opening input file");
    let mut source = WavSource::<_, Complex<i16>>::from_path(&input)?.convert();
    tracing::debug!(
        sample_rate = source.sample_rate(),
        length = source.len(),
        duration = %humantime::format_duration(Duration::from_secs_f32(source.sample_rate() * source.len() as f32))
    );

    tracing::info!(?listen_address, "Listening for connections");
    let listener = TcpListener::bind(listen_address).await?;
    let server = Server::from_listener(listener);

    let dongle_info = DongleInfo {
        tuner_type: TunerType::UNKNOWN,
        tuner_gain_count: 1,
    };

    let mut buffer = vec![Complex::ZERO; 0x4000].into_boxed_slice();

    crossterm::terminal::enable_raw_mode()?;
    let _terminal = Terminal::with_options(
        CrosstermBackend::new(std::io::stdout()),
        TerminalOptions {
            viewport: ratatui::Viewport::Inline(1),
        },
    )?;

    // filter terminal events for key presses and stop the stream when Q or ESC
    // is pressed
    let mut keys = Keys::new();

    println!("Press Q or ESC to quit");

    run_til_shutdown::<Result<(), Error>>(async move {
        loop {
            tokio::select! {
                result = keys.try_next() => {
                    let Some(_key) = result? else { break; };
                },
                result = server.wait() => {
                    let connection = result?.accept(dongle_info, Default::default()).await?;
                    tracing::info!(address = ?connection.address, "Client connected");

                    match handle_connection(
                        connection,
                        &mut source,
                        &mut buffer,
                        &mut keys,
                        Duration::from_secs(5),
                    )
                    .await {
                        Ok(true) => {},
                        Ok(false) => break,
                        Err(error) => tracing::error!(%error),
                    }
                }
            }
        }

        Ok(())
    })
    .await
    .transpose()?;

    if crossterm::terminal::is_raw_mode_enabled()? {
        crossterm::terminal::disable_raw_mode()?;
    }

    Ok(())
}

async fn handle_connection(
    mut connection: Connection,
    source: &mut Converted<WavSource<BufReader<File>, Complex<i16>>, Complex<u8>>,
    buffer: &mut [Complex<u8>],
    keys: &mut Keys,
    seek_time: Duration,
) -> Result<bool, Error> {
    // note: we could just use `source.forward(connection.sink)`, but we need to
    // rewrite `forward` a bit to allow us to pass it a buffer.
    //
    // <https://github.com/jgraef/mrrp/issues/45>

    let num_samples = source.len();

    /*let progress_bar = ProgressBar::new(num_samples as u64);
    progress_bar.set_style(
        ProgressStyle::with_template("{spinner:.green} {percent} % [{wide_bar:.cyan/blue}] ")
            .unwrap(),
    );*/

    let mut buffer_read_pos = 0;
    let mut buffer_write_pos = 0;
    let buffer_capacity = buffer.len();

    let sample_rate = source.sample_rate();
    let seek_delta = (seek_time.as_secs_f32() * sample_rate) as isize;
    let mut paused = false;

    let sample_duration = Duration::from_secs_f32(1.0 / sample_rate);
    let mut sink = connection.sink.throttled(sample_duration);

    loop {
        assert!(buffer_read_pos <= buffer_write_pos);
        assert!(
            (buffer_read_pos == 0 && buffer_write_pos == 0) || buffer_read_pos != buffer_write_pos
        );
        let (read_buffer, write_buffer) = buffer.split_at_mut(buffer_write_pos);
        let read_buffer = &read_buffer[buffer_read_pos..];

        tokio::select! {
            biased;

            result = keys.try_next() => {
                let Some(key) = result? else { return Ok(false); };
                tracing::debug!(?key, "key pressed");

                let mut clear_buffer = false;

                let mut seek = |seek_delta: isize| -> Result<(), Error> {
                    let wav = source.get_mut();
                    let position = wav.stream_position();
                    let new_position  = position.wrapping_add_signed(seek_delta) % num_samples;

                    tracing::debug!(?position, length = ?num_samples, ?new_position, "seeking");

                    wav.seek(new_position)?;
                    clear_buffer = true;
                    //progress_bar.set_position(new_position as u64);

                    Ok(())
                };

                match key {
                    KeyCode::Left => seek(-seek_delta)?,
                    KeyCode::Right => seek(seek_delta)?,
                    KeyCode::Char(' ') => paused ^= true,
                    _ => {}
                }

                if clear_buffer {
                    buffer_read_pos = 0;
                    buffer_write_pos = 0;

                }
            }
            result = connection.command_receiver.try_next() => {
                let Some(command) = result? else { break; };
                tracing::info!(?command, "Ignoring command");
            }
            result = sink.write(&read_buffer), if buffer_read_pos < buffer_write_pos => {
                let num_samples_written = match result {
                    Ok(n) => n,
                    Err(mrrp_rtl_tcp::server::Error::Io(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => {
                        break;
                    }
                    Err(error) => return Err(error.into()),
                };
                assert_ne!(num_samples_written, 0);

                buffer_read_pos += num_samples_written;
                if buffer_read_pos == buffer_write_pos {
                    buffer_read_pos = 0;
                    buffer_write_pos = 0;
                }
            }
            result = source.read_samples(write_buffer), if buffer_write_pos < buffer_capacity && !paused => {
                let num_samples_read = result?;
                if num_samples_read == 0 {
                    tracing::debug!("end of file. seeking to start");
                    source.get_mut().seek(0)?;
                };
                buffer_write_pos += num_samples_read;
                //progress_bar.inc(num_samples_read as u64);
            }
        }
    }

    tracing::info!("Client disconnected");

    Ok(true)
}

struct Keys {
    events: EventStream,
}

impl Keys {
    pub fn new() -> Self {
        let events = EventStream::new();
        Self { events }
    }
}

impl Stream for Keys {
    type Item = Result<KeyCode, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.events.poll_next_unpin(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error.into()))),
                Poll::Ready(Some(Ok(Event::Key(KeyEvent {
                    code: KeyCode::Char('q') | KeyCode::Esc,
                    kind: KeyEventKind::Press,
                    ..
                })))) => {
                    tracing::info!("Q or ESC pressed. Quitting.");
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(Ok(Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    ..
                })))) => {
                    return Poll::Ready(Some(Ok(code)));
                }
                _ => {}
            }
        }
    }
}
