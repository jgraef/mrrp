use mrrp_widgets::{
    spectrum::SpectrumState,
    waterfall::{
        WaterfallLine,
        WaterfallState,
    },
};

pub trait SpectrumSink {
    fn push(&mut self, frame: &SpectrumFrame);
}

impl SpectrumSink for SpectrumState {
    fn push(&mut self, frame: &SpectrumFrame) {
        let mut guard = self.update();
        let data = guard.data_mut();
        data.clear();
        data.extend(&frame.data);
    }
}

impl SpectrumSink for WaterfallState {
    fn push(&mut self, frame: &SpectrumFrame) {
        let mut guard = self.update();

        let (start_frequency, end_frequency) = frame.frequency_range();

        // todo: we can get rid of this clone if we could write directly to the staging
        // buffer
        let data = frame.data.clone();

        guard.push(WaterfallLine {
            data,
            start_frequency,
            end_frequency,
        });
    }
}

#[derive(Clone, Debug)]
pub struct SpectrumFrame {
    pub center_frequency: u64,
    pub sample_rate: u64,
    pub data: Vec<f32>,
}

impl SpectrumFrame {
    pub fn frequency_range(&self) -> (f32, f32) {
        let c = self.center_frequency as f32;
        let d = self.sample_rate as f32 / 2.0;
        (c - d, c + d)
    }
}
