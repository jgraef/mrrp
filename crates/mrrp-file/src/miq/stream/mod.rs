pub mod writer;

use chrono::{
    DateTime,
    Utc,
};
pub use mrrp_geo::{
    Geodetic,
    Orientation,
};
use num_complex::Complex;
use serde::{
    Deserialize,
    Serialize,
};

use crate::miq::container::chunk::{
    Tag,
    Tagged,
};

/// Stream ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StreamId(pub(super) u32);

/// Start of a stream
///
/// After this chunk the stream with ID `stream_id` is valid.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamStart<U = ()> {
    pub stream_id: StreamId,

    pub sample_format: SampleFormat,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<u64>,

    pub stream_info: StreamInfo<U>,
}

impl Tagged for StreamStart {
    const TAG: Tag = Tag::from_bytes_unchecked(*b"SSTA");
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamInfoChange<U = ()> {
    pub stream_id: StreamId,
    pub stream_info: StreamInfo<U>,
}

impl<U> Tagged for StreamInfoChange<U> {
    const TAG: Tag = Tag::from_bytes_unchecked(*b"SICH");
}

/// Tag for IQ data chunks
pub const IQ_TAG: Tag = Tag::from_bytes_unchecked(*b"SIQD");

/// End of a stream
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamEnd<U = ()> {
    pub stream_id: StreamId,
    pub stream_info: StreamInfo<U>,
}

impl Tagged for StreamEnd {
    const TAG: Tag = Tag::from_bytes_unchecked(*b"SEND");
}

macro_rules! make_stream_info {
    {$(
        $(#[$meta:meta])*
        $field:ident: $ty:ty as $as:tt,
    )*} => {
        // unfortunately we can't expand into struct fields, so we have to do a push-down accumulation
        //
        // https://fprijate.github.io/tlborm/pat-push-down-accumulation.html
        make_stream_info!(@make_struct([$(([$($meta),*], $field, $ty, $as),)*], {}));

        impl StreamInfo {
            /// Merge `other` into `self`.
            ///
            /// If a field in `self` is already set, it takes priority. Said differently: Any *empty* field is overwritten by the field from `other`.
            /// A field is empty if it's `None` or `false` depending on its type. This is the same condition used for [`is_empty`](Self::is_empty).
            pub fn merge(&mut self, other: Self) {
                $(
                    if make_stream_info!(@field_empty(self, $field, $as)) {
                        self.$field = other.$field;
                    }
                )*
            }

            /// Merge `other` into `self`.
            ///
            /// If a field in `self` is already set, it takes priority. Said differently: Any *empty* field is overwritten by the field from `other`.
            /// A field is empty if it's `None` or `false` depending on its type. This is the same condition used for [`is_empty`](Self::is_empty).
            pub fn merge_ref(&mut self, other: &Self) {
                $(
                    if make_stream_info!(@field_empty(self, $field, $as)) {
                        self.$field = other.$field.clone();
                    }
                )*
            }

            /// Returns `true` if all fields are empty.
            ///
            /// A field is empty if it's `None` or `false` depending on its type. This is the same condition used for merging.
            pub fn is_empty(&self) -> bool {
                $(
                    make_stream_info!(@field_empty(self, $field, $as))
                )&&*
            }
        }

        impl<U> Default for StreamInfo<U> {
            fn default() -> Self {
                Self {
                    $(
                        $field: Default::default(),
                    )*
                    user_defined: None,
                }
            }
        }
    };
    (@make_struct([], {$($output:tt)*})) => {
        #[doc = "Metadata for a stream"]
        #[doc = ""]
        #[doc = "This is defined in [`StreamDefinition`]. Changes can be encoded later"]
        #[doc = "in-band with [`MetadataChange`] chunks"]
        #[derive(Clone, Debug, Serialize, Deserialize)]
        // for some reason serde wants U to be Default, but it doesn't actually need to be.
        #[serde(bound(deserialize = "U: Deserialize<'de>"))]
        pub struct StreamInfo<U = ()> {
            $($output)*

            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub user_defined: Option<U>,
        }
    };
    (@make_struct([([$($meta:meta),*], $field:ident, $ty:ty, Option), $($rest:tt)*], {$($output:tt)*})) => {
        make_stream_info!(@make_struct([$($rest)*], {
            $($output)*
            $(#[$meta])*
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub $field: Option<$ty>,
        }));
    };
    (@make_struct([([$($meta:meta),*], $field:ident, $ty:ty, bool), $($rest:tt)*], {$($output:tt)*})) => {
        make_stream_info!(@make_struct([$($rest)*], {
            $($output)*
            $(#[$meta])*
            #[serde(default, skip_serializing_if = "std::ops::Not::not")]
            pub $field: bool,
        }));
    };
    (@field_empty($self:ident, $field:ident, Option)) => {
        $self.$field.is_none()
    };
    (@field_empty($self:ident, $field:ident, bool)) => {
        !$self.$field
    };
}

make_stream_info! {
    center_frequency: f32 as Option,
    sample_rate: f32 as Option,
    gain: f32 as Option,
    agc: Agc as Option,

    /// SM2117 "Data set unit"

    sample_unit: String as Option,

    /// SM2117 "Data set scaling factor"
    sample_scaling_factor: f32 as Option,

    comment: String as Option,
    device: String as Option,
    filter_bandwidth: f32 as Option,
    timestamp: DateTime<Utc> as Option,
    geo_location: Geodetic as Option,
    orientation: Orientation as Option,

    /// At least one sample might be invalid.
    sample_invalid: bool as bool,

    pll_unlocked: Pll as Option,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SampleFormat {
    Real(SampleComponentFormat),
    Iq(SampleComponentFormat),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SampleComponentFormat {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Agc {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Pll {
    Locked,
    Unlocked,
}

pub trait Sample {
    const SAMPLE_FORMAT: SampleFormat;
}

macro_rules! impl_sample {
    {$($ty:ty => $variant:ident,)*} => {
        $(
            impl Sample for $ty {
                const SAMPLE_FORMAT: SampleFormat = SampleFormat::Real(SampleComponentFormat::$variant);
            }

            impl Sample for Complex<$ty> {
                const SAMPLE_FORMAT: SampleFormat = SampleFormat::Iq(SampleComponentFormat::$variant);
            }
        )*
    };
}

impl_sample! {
    u8 => U8,
    i8 => I8,
    u16 => U16,
    i16 => I16,
    u32 => U32,
    i32 => I32,
    u64 => U64,
    i64 => I64,
    f32 => F32,
    f64 => F64,
}
