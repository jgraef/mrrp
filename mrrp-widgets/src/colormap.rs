use std::{
    borrow::Cow,
    sync::{
        Arc,
        OnceLock,
    },
};

use parking_lot::RwLock;

pub type Color = [f32; 4];

#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize),
    serde(from = "serde_impl::DeserializeHelper",)
)]
pub struct ColorMap {
    inner: Arc<Inner>,
}

impl ColorMap {
    pub fn new(lut: impl Into<Cow<'static, [Color]>>) -> Self {
        Self {
            inner: Arc::new(Inner {
                lut: lut.into(),
                state: RwLock::new(State { buffer: None }),
            }),
        }
    }

    pub fn from_fn(num_samples: usize, mut f: impl FnMut(f32) -> Color) -> Self {
        let s = 1.0 / (num_samples - 1) as f32;

        let mut lut = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = s * i as f32;
            let color = f(t);
            lut.push(color);
        }

        Self::new(lut)
    }

    #[cfg(feature = "colorgrad")]
    pub fn from_colograd(num_samples: usize, gradient: impl colorgrad::Gradient) -> Self {
        Self::from_fn(num_samples, |t| gradient.at(t).to_array())
    }

    pub(crate) fn buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        // note: read-upgradable locking this won't help since only one thread can do
        // this at once

        // optimitically we only need to read
        let guard = self.inner.state.read();

        if let Some(buffer) = &guard.buffer {
            return buffer.clone();
        }

        // no luck, we first need to upload the data to the gpu buffer.

        // drop guard and acquire write-guard
        drop(guard);
        let mut guard = self.inner.state.write();

        // just in case someone uploaded the data inbetween us switching from ro to rw
        // lock
        if let Some(buffer) = &guard.buffer {
            return buffer.clone();
        }

        // ship it

        let lut_bytes = bytemuck::cast_slice::<_, u8>(&*self.inner.lut);

        let size = lut_bytes.len().try_into().unwrap();
        tracing::debug!(
            ?size,
            lut_entries = self.inner.lut.len(),
            "Creating colormap buffer"
        );

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("colormap"),
            size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: true,
        });

        {
            let mut view_mut = buffer.get_mapped_range_mut(..);
            view_mut.copy_from_slice(lut_bytes);
        }

        buffer.unmap();

        // don't forget to remember the buffer :)
        guard.buffer = Some(buffer.clone());

        buffer
    }

    pub fn lut(&self) -> &[[f32; 4]] {
        &self.inner.lut
    }
}

impl Default for ColorMap {
    /// Inferno color map with 32 samples.
    ///
    /// We recommend you pick your own from the [`colorgrad`] crate (don't
    /// forget to enable the `colorgrad` feature). This is just so we can
    /// provide any default.
    fn default() -> Self {
        // we want to always return clormaps that share the same internal state so that
        // they all share the same gpu buffers

        static DEFAULT: OnceLock<ColorMap> = OnceLock::new();
        DEFAULT.get_or_init(|| Self::new(DEFAULT_LUT)).clone()
    }
}

#[derive(Debug)]
struct Inner {
    //metadata: Metadata,
    lut: Cow<'static, [Color]>,
    state: RwLock<State>,
}

#[derive(Debug)]
struct State {
    buffer: Option<wgpu::Buffer>,
}

#[cfg(feature = "serde")]
mod serde_impl {
    use serde::{
        Deserialize,
        Serialize,
    };

    use crate::colormap::{
        Color,
        ColorMap,
    };

    #[derive(Deserialize)]
    #[serde(rename = "ColorMap")]
    pub struct DeserializeHelper {
        lut: Vec<Color>,
    }

    impl From<DeserializeHelper> for ColorMap {
        fn from(value: DeserializeHelper) -> Self {
            ColorMap::new(value.lut)
        }
    }

    #[derive(Serialize)]
    #[serde(rename = "ColorMap")]
    pub struct SerializeHelper<'a> {
        lut: &'a [Color],
    }

    impl Serialize for ColorMap {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            SerializeHelper {
                lut: &self.inner.lut,
            }
            .serialize(serializer)
        }
    }
}

/// We need to provide some kind of default. This is inferno with 32 samples
const DEFAULT_LUT: &[[f32; 4]] = &[
    [0.0, 0.0, 0.015686275, 1.0],
    [0.029534295, 0.013651972, 0.08382195, 0.99999994],
    [0.061701313, 0.025724312, 0.15090455, 1.0],
    [0.09913377, 0.034637388, 0.21588098, 1.0],
    [0.14379264, 0.039419327, 0.27731425, 1.0],
    [0.19400565, 0.04238553, 0.33169135, 1.0],
    [0.24687767, 0.046957683, 0.37480035, 0.99999994],
    [0.29977116, 0.056131482, 0.4030918, 1.0],
    [0.3519578, 0.06974791, 0.417914, 1.0],
    [0.40356526, 0.086234, 0.4228109, 1.0],
    [0.45472518, 0.1040929, 0.4211409, 1.0],
    [0.5055691, 0.122682326, 0.4142427, 1.0],
    [0.5562288, 0.1418641, 0.4022634, 1.0],
    [0.6068027, 0.16155873, 0.38535354, 0.99999994],
    [0.65689284, 0.1824667, 0.3639473, 1.0],
    [0.70571923, 0.20588893, 0.33869734, 1.0],
    [0.75249213, 0.23311432, 0.3102508, 0.99999994],
    [0.7964216, 0.26480106, 0.27900264, 1.0],
    [0.8367181, 0.3009766, 0.24509537, 1.0],
    [0.8725986, 0.34163675, 0.20869021, 0.99999994],
    [0.9035532, 0.3866135, 0.17109449, 1.0],
    [0.9294267, 0.4355261, 0.1351048, 1.0],
    [0.9500869, 0.48797867, 0.10363832, 1.0],
    [0.9653556, 0.5434832, 0.08126237, 0.99999994],
    [0.974977, 0.6013966, 0.07534096, 1.0],
    [0.97869325, 0.6610598, 0.09349236, 1.0],
    [0.9773248, 0.7215903, 0.13983747, 0.99999994],
    [0.9741004, 0.78160733, 0.21069328, 1.0],
    [0.9725732, 0.8396633, 0.30132064, 1.0],
    [0.97516125, 0.894864, 0.40692243, 1.0],
    [0.9809084, 0.94795847, 0.52252877, 1.0],
    [0.9882353, 1.0, 0.6431373, 1.0],
];
