use std::time::Duration;

use rand::rngs::SmallRng;
use rand_distr::{
    Distribution,
    Normal,
};
use tokio::sync::watch;

use crate::sdr::sink::SpectrumFrame;

#[derive(Clone, Debug)]
pub struct MockSpectrumSource {
    receiver: watch::Receiver<SpectrumFrame>,
}

impl MockSpectrumSource {
    pub fn mock(center_frequency: u64, sample_rate: u64, buffer_size: usize) -> Self {
        let mut rng: SmallRng = rand::make_rng();
        let noise = Normal::new(0.01, 0.003).unwrap();

        let mut fill_buffer = move |buffer: &mut Vec<f32>| {
            // clear and re-fill buffer with `buffer_size` number of values
            buffer.clear();
            buffer.resize_with(buffer_size, || {
                let value: f32 = noise.sample(&mut rng);
                value.clamp(0.001, 1.0)
            });
        };

        let mut data = vec![];
        fill_buffer(&mut data);

        let (sender, receiver) = watch::channel(SpectrumFrame {
            center_frequency,
            sample_rate,
            data,
        });

        tokio::spawn({
            async move {
                let mut interval = tokio::time::interval(Duration::from_secs_f32(
                    buffer_size as f32 / sample_rate as f32,
                ));

                while !sender.is_closed() {
                    interval.tick().await;

                    sender.send_modify(|frame| {
                        fill_buffer(&mut frame.data);
                    });
                }
            }
        });

        Self { receiver }
    }
}
