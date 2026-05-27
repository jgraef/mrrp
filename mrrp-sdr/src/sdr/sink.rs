use mrrp::buf::SampleBufMut;
use mrrp_widgets::{
    spectrum::SpectrumState,
    waterfall::{
        WaterfallLine,
        WaterfallState,
    },
};

// todo: move this into mrrp
pub trait SpectrumSink {
    fn push<'a>(&mut self, frame: SpectrumFrame<&'a [f32]>);
}

impl SpectrumSink for SpectrumState {
    fn push<'a>(&mut self, frame: SpectrumFrame<&'a [f32]>) {
        let mut guard = self.update();

        // ideally we would just clone the Samples and put it into the widget state, but
        // that would make mrrp-widgets depend on mrrp, which we try to avoid
        //
        // even better yet, we would expose an API that lets us write directly to the
        // staging buffer

        let data = guard.data_mut();
        data.clear();
        data.put(&*frame.data);
    }
}

impl SpectrumSink for WaterfallState {
    fn push<'a>(&mut self, frame: SpectrumFrame<&'a [f32]>) {
        let mut guard = self.update();

        let (start_frequency, end_frequency) = frame.frequency_range();

        // see comment in <SpectrumState as SpectrumSink>::push

        let mut data = vec![];
        data.put(&*frame.data);

        guard.push(WaterfallLine {
            data,
            start_frequency,
            end_frequency,
        });
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SpectrumFrame<B> {
    pub center_frequency: u64,
    pub sample_rate: u64,
    pub data: B,
}

impl<B> SpectrumFrame<B> {
    pub fn frequency_range(&self) -> (f32, f32) {
        let c = self.center_frequency as f32;
        let d = self.sample_rate as f32 / 2.0;
        (c - d, c + d)
    }
}
