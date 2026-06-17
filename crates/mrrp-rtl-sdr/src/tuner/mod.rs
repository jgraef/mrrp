pub mod r82xx;

use std::{
    any::Any,
    convert::Infallible,
    fmt::Debug,
    pin::Pin,
};

use futures_util::TryFutureExt;

use crate::{
    rtl2832u::{
        IfMode,
        Rtl2832u,
    },
    tuner::gain::TunerGain,
};

pub trait TunerError: std::error::Error + Send + Sync + Sized + 'static {}

impl TunerError for Infallible {}

pub trait TunerProbe: Clone + Debug + Sized + Send + Sync + 'static {
    type Error: TunerError;
    type Tuner: Tuner;

    /// Probe for a tuner and return its initial state if one is found
    ///
    /// # Note
    ///
    /// The I2C repeater must be enabled by the caller.
    fn try_open<'a>(
        &'a self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> impl Future<Output = Result<Option<Self::Tuner>, Self::Error>> + Send + 'a;
}

pub trait Tuner: Debug + Sized + Send + Sync + 'static {
    type Error: TunerError;

    fn name<'a>(&'a self) -> &'a str;

    fn set_bandwidth<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        bandwidth: f32,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn set_center_frequency<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        center_frequency: f32,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn if_setting(&self) -> IfSetting;

    /// Return a list of available gains in dB.
    ///
    /// This list is sorted in ascending order.
    fn gains(&self) -> &[f32];

    /// Set tuner gain
    ///
    /// The provided index (if not auto), refers to the gains returned by
    /// [`gains`][Self::gains].
    fn set_gain<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        gain: TunerGain,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn shutdown<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct AnyTunerError(Box<dyn std::error::Error + Send + Sync>);

impl AnyTunerError {
    pub fn new(error: impl TunerError) -> Self {
        Self(Box::new(error))
    }
}

impl TunerError for AnyTunerError {}

#[derive(Clone, Debug)]
pub struct FallbackTunerProbe;

impl TunerProbe for FallbackTunerProbe {
    type Error = AnyTunerError;
    type Tuner = AnyTuner;

    async fn try_open(&self, rtl2832u: &mut Rtl2832u) -> Result<Option<Self::Tuner>, Self::Error> {
        macro_rules! probe {
                {$($probe:expr,)*} => {
                    $(
                        if let Some(tuner) = $probe
                            .try_open(rtl2832u)
                            .await
                            .map_err(AnyTunerError::new)?
                        {
                            return Ok(Some(AnyTuner::new(tuner)));
                        }
                    )*
                };
            }

        probe! {
            r82xx::R82xxProbe,
        }

        Ok(None)
    }
}

trait AnyTunerProbeTrait: Debug + Send + Sync + 'static {
    fn any_clone(&self) -> AnyTunerProbe;

    fn any_try_open<'a>(
        &'a self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> Pin<Box<dyn Future<Output = Result<Option<AnyTuner>, AnyTunerError>> + Send + 'a>>;
}

impl<T> AnyTunerProbeTrait for T
where
    T: TunerProbe,
{
    fn any_clone(&self) -> AnyTunerProbe {
        AnyTunerProbe::new(self.clone())
    }

    fn any_try_open<'a>(
        &'a self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> Pin<Box<dyn Future<Output = Result<Option<AnyTuner>, AnyTunerError>> + Send + 'a>> {
        Box::pin(
            self.try_open(rtl2832u)
                .map_ok(|tuner_opt| tuner_opt.map(AnyTuner::new))
                .map_err(AnyTunerError::new),
        )
    }
}

pub struct AnyTunerProbe(Box<dyn AnyTunerProbeTrait>);

impl AnyTunerProbe {
    pub fn new(probe: impl TunerProbe) -> Self {
        Self(Box::new(probe))
    }
}

impl Debug for AnyTunerProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Clone for AnyTunerProbe {
    fn clone(&self) -> Self {
        self.0.any_clone()
    }
}

impl TunerProbe for AnyTunerProbe {
    type Error = AnyTunerError;
    type Tuner = AnyTuner;

    fn try_open<'a>(
        &'a self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> impl Future<Output = Result<Option<Self::Tuner>, Self::Error>> + Send + 'a {
        self.0.any_try_open(rtl2832u)
    }
}

trait AnyTunerTrait: Debug + Send + Sync + Any {
    fn name(&self) -> &str;

    fn set_bandwidth<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        bandwidth: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>>;

    fn set_center_frequency<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        center_frequency: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>>;

    fn if_setting(&self) -> IfSetting;

    fn gains(&self) -> &[f32];

    fn set_gain<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        gain: TunerGain,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>>;

    fn shutdown<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>>;
}

impl<T> AnyTunerTrait for T
where
    T: Tuner,
{
    fn name(&self) -> &str {
        Tuner::name(self)
    }

    fn set_bandwidth<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        bandwidth: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>> {
        Box::pin(Tuner::set_bandwidth(self, rtl2832u, bandwidth).map_err(AnyTunerError::new))
    }

    fn set_center_frequency<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        center_frequency: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>> {
        Box::pin(
            Tuner::set_center_frequency(self, rtl2832u, center_frequency)
                .map_err(AnyTunerError::new),
        )
    }

    fn if_setting(&self) -> IfSetting {
        Tuner::if_setting(self)
    }

    fn gains(&self) -> &[f32] {
        Tuner::gains(self)
    }

    fn set_gain<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        gain: TunerGain,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>> {
        Box::pin(Tuner::set_gain(self, rtl2832u, gain).map_err(AnyTunerError::new))
    }

    fn shutdown<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>> {
        Box::pin(Tuner::shutdown(self, rtl2832u).map_err(AnyTunerError::new))
    }
}

pub struct AnyTuner(Box<dyn AnyTunerTrait>);

impl AnyTuner {
    #[inline(always)]
    pub fn new(tuner: impl Tuner) -> Self {
        Self(Box::new(tuner))
    }

    #[inline(always)]
    pub fn downcast_ref<T>(&self) -> Option<&T>
    where
        T: Tuner,
    {
        (&*self.0 as &dyn Any).downcast_ref::<T>()
    }

    #[inline(always)]
    pub fn downcast_mut<T>(&mut self) -> Option<&mut T>
    where
        T: Tuner,
    {
        (&mut *self.0 as &mut dyn Any).downcast_mut::<T>()
    }
}

impl Debug for AnyTuner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Tuner for AnyTuner {
    type Error = AnyTunerError;

    fn name(&self) -> &str {
        self.0.name()
    }

    fn set_bandwidth<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        bandwidth: f32,
    ) -> impl Future<Output = Result<(), Self::Error>> + 'a {
        self.0.set_bandwidth(rtl2832u, bandwidth)
    }

    fn set_center_frequency<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        center_frequency: f32,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.0.set_center_frequency(rtl2832u, center_frequency)
    }

    fn if_setting(&self) -> IfSetting {
        self.0.if_setting()
    }

    fn gains(&self) -> &[f32] {
        self.0.gains()
    }

    fn set_gain<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        gain: TunerGain,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.0.set_gain(rtl2832u, gain)
    }

    fn shutdown<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.0.shutdown(rtl2832u)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NullTuner;

impl Tuner for NullTuner {
    type Error = Infallible;

    fn name(&self) -> &str {
        "null"
    }

    async fn set_bandwidth<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        bandwidth: f32,
    ) -> Result<(), Self::Error> {
        let _ = (rtl2832u, bandwidth);
        Ok(())
    }

    async fn set_center_frequency<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        center_frequency: f32,
    ) -> Result<(), Self::Error> {
        let _ = (rtl2832u, center_frequency);
        Ok(())
    }

    fn if_setting(&self) -> IfSetting {
        IfSetting::ZeroIf
    }

    fn gains(&self) -> &[f32] {
        &[]
    }

    async fn set_gain<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        gain: TunerGain,
    ) -> Result<(), Self::Error> {
        let _ = (rtl2832u, gain);
        Ok(())
    }

    async fn shutdown<'a>(&'a mut self, rtl2832u: &'a mut Rtl2832u) -> Result<(), Self::Error> {
        let _ = rtl2832u;
        Ok(())
    }
}

impl TunerProbe for NullTuner {
    type Error = Infallible;
    type Tuner = Self;

    fn try_open<'a>(
        &'a self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> impl Future<Output = Result<Option<Self::Tuner>, Self::Error>> + Send + 'a {
        let _ = rtl2832u;
        std::future::ready(Ok(Some(Self)))
    }
}

/// If configuration of a tuner that is needed to configure the [`Rtl2832u`].
///
/// # TODO
///
/// - Does the [`ZeroIf`](Self::ZeroIf) need a variant specifying whether to
///   pick I or Q? The RTL2832U might expect this on the Q channel, but we can
///   swap them.
/// - Do we want the [`If`](Self::If) variant to be able to swap I and Q?
#[derive(Clone, Copy, Debug)]
pub enum IfSetting {
    ZeroIf,
    If {
        frequency: f32,
        invert_spectrum: bool,
    },
}

impl IfSetting {
    pub fn if_mode(&self) -> IfMode {
        match self {
            IfSetting::ZeroIf => IfMode::ZeroIf,
            IfSetting::If { .. } => IfMode::If,
        }
    }
}

pub mod gain {
    pub trait IntoTunerGain {
        fn into_tuner_gain(self, available_gains: &[f32]) -> TunerGain;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum TunerGain {
        Auto,
        Manual(usize),
    }

    impl TunerGain {
        #[inline(always)]
        pub fn from_db(gain: f32, available_gains: &[f32]) -> Self {
            Index::from_db(gain, available_gains).into_tuner_gain(available_gains)
        }
    }

    impl IntoTunerGain for TunerGain {
        #[inline(always)]
        fn into_tuner_gain(self, available_gains: &[f32]) -> TunerGain {
            let _ = available_gains;
            self
        }
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct Auto;

    impl IntoTunerGain for Auto {
        /// Convert to [`TunerGain`] enum.
        ///
        /// The `available_gains` provides the gain values in dB that are
        /// available. It is sorted in ascending order. This is needed by the
        /// conversion from dB values to the [`TunerGain`] enum, as it only
        /// understands indices.
        #[inline(always)]
        fn into_tuner_gain(self, available_gains: &[f32]) -> TunerGain {
            let _ = available_gains;
            TunerGain::Auto
        }
    }

    #[derive(Clone, Copy, Debug, Default, derive_more::From, derive_more::Into)]
    pub struct Index(pub usize);

    impl Index {
        #[inline(always)]
        pub fn from_db(gain: f32, available_gains: &[f32]) -> Self {
            let index = closest_gain(gain, available_gains);

            // if `gains` is empty, this will return `None`. Should we return an error in
            // that case?
            let index = index.unwrap_or_default();

            Self(index)
        }
    }

    impl IntoTunerGain for Index {
        #[inline(always)]
        fn into_tuner_gain(self, available_gains: &[f32]) -> TunerGain {
            let _ = available_gains;
            TunerGain::Manual(self.0)
        }
    }

    #[derive(Clone, Copy, Debug, Default, derive_more::From, derive_more::Into)]
    pub struct Db(pub f32);

    impl IntoTunerGain for Db {
        #[inline(always)]
        fn into_tuner_gain(self, available_gains: &[f32]) -> TunerGain {
            Index::from_db(self.0, available_gains).into_tuner_gain(available_gains)
        }
    }

    /// Finds the index of the closest gain value.
    ///
    /// Returns `None` if the provided `gains` array is empty.
    pub fn closest_gain(gain: f32, available_gains: &[f32]) -> Option<usize> {
        match available_gains.binary_search_by(|other| {
            other
                .partial_cmp(&gain)
                .unwrap_or_else(|| panic!("Can't compare floats: {} and {}", other, gain))
        }) {
            Ok(index) => Some(index),
            Err(index_after) => {
                assert!(index_after <= available_gains.len());

                if index_after == available_gains.len() {
                    // value is larger than the last gain entry, so return that.
                    //
                    // there's the edge case that the gains array is empty. in that case we return
                    // `None`
                    index_after.checked_sub(1)
                }
                else if let Some(index_before) = index_after.checked_sub(1) {
                    // value is less than `index_after`, but greater than `index_before`. check
                    // which one is closer
                    let distance_before = gain - available_gains[index_before];
                    let distance_after = available_gains[index_after] - gain;
                    assert!(distance_before > 0.0);
                    assert!(distance_after > 0.0);

                    if distance_before < distance_after {
                        Some(index_before)
                    }
                    else {
                        Some(index_after)
                    }
                }
                else {
                    // value is smaller than the first gain entry, so return that
                    Some(index_after)
                }
            }
        }
    }
}
