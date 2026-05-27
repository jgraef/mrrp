use std::{
    sync::Arc,
    time::Duration,
};

use mrrp_widgets::spectrum::{
    SpectrumState,
    SpectrumView,
};
use rand::{
    distr::Distribution,
    rngs::SmallRng,
};
use rand_distr::Normal;
use serde::{
    Deserialize,
    Serialize,
};
use tokio_util::task::AbortOnDropHandle;

#[derive(Debug)]
pub struct SpectrumDock<'a> {
    state: &'a mut SpectrumDockState,
}

impl<'a> SpectrumDock<'a> {
    pub fn new(state: &'a mut SpectrumDockState) -> Self {
        Self { state }
    }

    pub fn show(self, ui: &mut egui::Ui) {
        ui.add(SpectrumView::new(&self.state.inner(ui.ctx()).state));
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpectrumDockState {
    /// Holds state for the dock, this is not serializable, so it'll be lazily
    /// initialized when needed.
    #[serde(skip, default)]
    inner: Option<StateInner>,
}

impl SpectrumDockState {
    fn inner(&mut self, ctx: &egui::Context) -> &mut StateInner {
        self.inner
            .get_or_insert_with(|| StateInner::new(ctx.clone()))
    }
}

#[derive(Clone, Debug)]
struct StateInner {
    /// When this drops the task that generates the test data is cancelled
    _drop_handle: Arc<AbortOnDropHandle<()>>,

    /// Holds GPU resources (pipeline, buffers, etc.)
    state: SpectrumState,
}

impl StateInner {
    fn new(ctx: egui::Context) -> Self {
        // for testing we spawn a task that generates random data
        //let center_frequency = 7_000_000;
        let sample_rate = 2_400_000;
        let buffer_size = 4096;

        let state = SpectrumState::default();

        let join_handle = tokio::spawn({
            let state = state.clone();

            async move {
                let mut rng: SmallRng = rand::make_rng();
                let noise = Normal::new(0.01, 0.003).unwrap();

                let mut interval = tokio::time::interval(Duration::from_secs_f32(
                    buffer_size as f32 / sample_rate as f32,
                ));

                loop {
                    interval.tick().await;

                    // clear and re-fill buffer with `buffer_size` number of values
                    let mut update_guard = state.update();
                    let buffer = update_guard.data_mut();
                    buffer.clear();
                    buffer.resize_with(buffer_size, || {
                        let value: f32 = noise.sample(&mut rng);
                        value.clamp(0.001, 1.0)
                    });

                    // trigger repaint
                    ctx.request_repaint();
                }
            }
        });

        Self {
            _drop_handle: Arc::new(AbortOnDropHandle::new(join_handle)),
            state,
        }
    }
}
