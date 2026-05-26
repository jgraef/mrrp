use std::{
    sync::Arc,
    time::Duration,
};

use mrrp_widgets::spectrum::{
    SpectrumState,
    SpectrumView,
};
use rand::{
    RngExt,
    rngs::SmallRng,
};
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
        ui.add(SpectrumView::new(&self.state.inner().state));
        ui.label("TODO: Spectrum");
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
    fn inner(&mut self) -> &mut StateInner {
        self.inner.get_or_insert_with(|| StateInner::new())
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
    fn new() -> Self {
        // for testing we spawn a task that generates random data
        //let center_frequency = 7_000_000;
        let sample_rate = 2_400_000;
        let buffer_size = 4096;

        let mut data = vec![0.0; buffer_size];

        let mut rng: SmallRng = rand::make_rng();

        let mut fill_uniform = move |data: &mut [f32]| {
            data.iter_mut().for_each(|value| {
                *value = rng.random(); // standard uniform [0, 1)
            });
        };

        fill_uniform(&mut data);

        let state = SpectrumState::default();

        let join_handle = tokio::spawn({
            let state = state.clone();

            async move {
                let mut interval = tokio::time::interval(Duration::from_secs_f32(
                    buffer_size as f32 / sample_rate as f32,
                ));

                loop {
                    interval.tick().await;

                    fill_uniform(state.update().data_mut());
                }
            }
        });

        Self {
            _drop_handle: Arc::new(AbortOnDropHandle::new(join_handle)),
            state,
        }
    }
}
