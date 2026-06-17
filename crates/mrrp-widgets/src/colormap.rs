use std::{
    borrow::Cow,
    sync::{
        Arc,
        OnceLock,
    },
};

use bytemuck::{
    Pod,
    Zeroable,
};
use parking_lot::RwLock;

/// The color we use.
///
/// It's linear RGBA.
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct Color(pub [f32; 4]);

#[cfg(feature = "colorgrad")]
impl From<colorgrad::Color> for Color {
    fn from(value: colorgrad::Color) -> Self {
        Self(value.to_linear_rgba())
    }
}

#[cfg(feature = "colorgrad")]
impl From<Color> for colorgrad::Color {
    fn from(value: Color) -> Self {
        Self::from_linear_rgba(value.0[0], value.0[1], value.0[2], value.0[3])
    }
}

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
        Self::from_fn(num_samples, |t| gradient.at(t).into())
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

    pub fn lut(&self) -> &[Color] {
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
const DEFAULT_LUT: &[Color] = &[
    Color([0.0, 0.0, 0.001214108, 1.0]),
    Color([0.0022859361, 0.0010566541, 0.0076929643, 0.99999994]),
    Color([0.00507196, 0.001991046, 0.01981492, 1.0]),
    Color([0.009888917, 0.0026809124, 0.038270354, 1.0]),
    Color([0.018211879, 0.0030510314, 0.06250455, 1.0]),
    Color([0.031267714, 0.0032853424, 0.0899226, 1.0000001]),
    Color([0.049635056, 0.0036677683, 0.115886904, 0.99999994]),
    Color([0.0731257, 0.004510276, 0.1350455, 1.0]),
    Color([0.10165122, 0.0059521506, 0.14577107, 1.0]),
    Color([0.13538073, 0.008017674, 0.14941998, 1.0000001]),
    Color([0.1745021, 0.010669792, 0.14816967, 1.0]),
    Color([0.21923205, 0.013910419, 0.14306988, 1.0]),
    Color([0.26982567, 0.017790731, 0.13446017, 1.0]),
    Color([0.32654467, 0.022365352, 0.12283375, 0.99999994]),
    Color([0.38903546, 0.027902339, 0.108987495, 1.0]),
    Color([0.45617628, 0.03496936, 0.093882374, 1.0]),
    Color([0.5264123, 0.044376157, 0.078417495, 0.99999994]),
    Color([0.5977799, 0.05700407, 0.06326942, 1.0]),
    Color([0.66794443, 0.07372345, 0.04893465, 1.0]),
    Color([0.73427427, 0.09557344, 0.035877295, 0.99999994]),
    Color([0.79446185, 0.12367894, 0.024802187, 1.0]),
    Color([0.8469042, 0.15914142, 0.016359782, 1.0]),
    Color([0.89019024, 0.20308262, 0.010596772, 1.0]),
    Color([0.92299175, 0.25651857, 0.007356924, 0.99999994]),
    Color([0.94401777, 0.3201794, 0.0066128443, 1.0]),
    Color([0.95221305, 0.3945231, 0.0090424055, 1.0000001]),
    Color([0.9491906, 0.47935227, 0.017354341, 0.99999994]),
    Color([0.9420908, 0.5731208, 0.036534864, 1.0]),
    Color([0.938739, 0.67325145, 0.07389457, 1.0]),
    Color([0.94442326, 0.77728736, 0.13777165, 1.0]),
    Color([0.9571176, 0.88567257, 0.23548913, 1.0]),
    Color([0.9734455, 1.0, 0.37123778, 1.0]),
];
