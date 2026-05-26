use std::time::Duration;

use rand::{
    RngExt,
    rngs::SmallRng,
};
use tokio::sync::watch;

use crate::ui::widgets::spectrum_cpu::{
    SpectrumData,
    SpectrumView,
};

#[derive(Debug)]
pub struct SpectrumDock {
    pub amplitude_spectrum: watch::Receiver<SpectrumData>,
}

impl SpectrumDock {
    pub fn new() -> Self {
        Self {
            amplitude_spectrum: test_data(),
        }
    }

    pub fn show(mut self, ui: &mut egui::Ui) {
        ui.add(SpectrumView::new(
            &self.amplitude_spectrum.borrow_and_update(),
        ));
        ui.label("TODO: Spectrum");
    }
}

fn test_data() -> watch::Receiver<SpectrumData> {
    // for testing we spawn a task that generates random data
    let center_frequency = 7_000_000;
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

    let (sender, receiver) = watch::channel(SpectrumData {
        data,
        start_frequency: center_frequency as f32 - sample_rate as f32 / 2.0,
        end_frequency: center_frequency as f32 + sample_rate as f32 / 2.0,
    });

    let _join_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs_f32(
            buffer_size as f32 / sample_rate as f32,
        ));

        while !sender.is_closed() {
            interval.tick().await;

            sender.send_modify(|data| {
                fill_uniform(&mut data.data);
            });
        }
    });

    receiver
}
