use std::time::Duration;

use chrono::{
    DateTime,
    TimeDelta,
    Utc,
};
use indicatif::{
    ProgressBar,
    ProgressStyle,
};

#[derive(Clone, Debug, Default)]
pub struct Options<'a> {
    pub progress_bar: Option<ProgressBarOptions<'a>>,
}

#[derive(Clone, derive_more::Debug)]
pub struct ProgressBarOptions<'a> {
    #[debug(skip)]
    pub style: ProgressStyle,
    pub message: &'a str,
}

impl Default for ProgressBarOptions<'static> {
    fn default() -> Self {
        Self {
            style: default_progress_style(),
            message: "waiting until",
        }
    }
}

pub async fn sleep_until(deadline: DateTime<Utc>) {
    sleep_until_with_options(deadline, Default::default()).await;
}

pub async fn sleep_until_with_progress(deadline: DateTime<Utc>, message: &str) {
    sleep_until_with_options(
        deadline,
        Options {
            progress_bar: Some(ProgressBarOptions {
                style: default_progress_style(),
                message,
            }),
        },
    )
    .await;
}

pub async fn sleep_until_with_options(deadline: DateTime<Utc>, options: Options<'_>) {
    let sleep_interval = if options.progress_bar.is_some() {
        // when rendering a progress bar we need to update it frequently
        Duration::from_millis(100)
    }
    else {
        // otherwise we sleep at most a minute at a time and then recheck if the
        // time has come. this is to make edge-cases somewhat work,
        // where the computer's time changes, e.g. when the user changes
        // it.
        Duration::from_mins(1)
    };

    let start_time = Utc::now();
    let total = deadline - start_time;

    if total <= TimeDelta::zero() {
        // early return
        return;
    }
    let total = total.to_std().unwrap();

    let mut progress_bar = options.progress_bar.map(|options| {
        let progress_bar = ProgressBar::new(total.as_secs());
        progress_bar
            .with_style(register_trackers(options.style, deadline))
            .with_message(options.message.to_owned())
    });

    loop {
        let now = Utc::now();

        let remaining = deadline - now;
        if remaining <= TimeDelta::zero() {
            // done waiting
            break;
        }

        let remaining = remaining.to_std().unwrap();

        if let Some(progress_bar) = &mut progress_bar {
            let position = total.saturating_sub(remaining).as_secs();
            progress_bar.set_position(position);
        }

        tokio::time::sleep(remaining.min(sleep_interval)).await;
    }

    if let Some(progress_bar) = &mut progress_bar {
        progress_bar.finish_and_clear();
    }
}

pub fn default_progress_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{spinner:.green} {msg} {deadline}; {remaining}; {percent} % [{wide_bar:.cyan/blue}]",
    )
    .unwrap()
}

pub fn register_trackers(style: ProgressStyle, deadline: DateTime<Utc>) -> ProgressStyle {
    style
        .with_key("deadline", tracker::Deadline { deadline })
        .with_key("remaining", tracker::Remaining)
}

mod tracker {
    use std::{
        fmt::Write,
        time::{
            Duration,
            Instant,
        },
    };

    use chrono::{
        DateTime,
        Utc,
    };
    use indicatif::{
        ProgressState,
        style::ProgressTracker,
    };

    #[derive(Clone, Copy, Debug)]
    pub struct Deadline {
        pub deadline: DateTime<Utc>,
    }

    impl ProgressTracker for Deadline {
        #[inline]
        fn clone_box(&self) -> Box<dyn ProgressTracker> {
            Box::new(self.clone())
        }

        fn tick(&mut self, state: &ProgressState, now: Instant) {
            let _ = (state, now);
            // nop
        }

        fn reset(&mut self, state: &ProgressState, now: Instant) {
            let _ = (state, now);
            // nop
        }

        fn write(&self, state: &ProgressState, w: &mut dyn Write) {
            let _ = state;
            write!(w, "{}", self.deadline.format("%Y-%m-%d %H:%M:%SZ")).unwrap()
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub struct Remaining;

    impl ProgressTracker for Remaining {
        #[inline]
        fn clone_box(&self) -> Box<dyn ProgressTracker> {
            Box::new(self.clone())
        }

        fn tick(&mut self, state: &ProgressState, now: Instant) {
            let _ = (state, now);
            // nop
        }

        fn reset(&mut self, state: &ProgressState, now: Instant) {
            let _ = (state, now);
            // nop
        }

        fn write(&self, state: &ProgressState, w: &mut dyn Write) {
            if let Some(len) = state.len() {
                let remaining = Duration::from_secs(len.saturating_sub(state.pos()));
                write!(w, "{}", humantime::format_duration(remaining)).unwrap()
            }
            else {
                write!(w, "?").unwrap()
            }
        }
    }
}
