pub mod blog;
pub mod r82xx;

use std::{
    convert::Infallible,
    fmt::Debug,
    pin::Pin,
};

use futures_util::TryFutureExt;

use crate::rtl2832u::Rtl2832u;

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
        rtl2832u: &'a Rtl2832u,
    ) -> impl Future<Output = Result<Option<Self::Tuner>, Self::Error>> + Send + 'a;
}

pub trait Tuner: Debug + Sized + Send + Sync + 'static {
    type Error: TunerError;

    fn name<'a>(&'a self) -> &'a str;

    fn set_bandwidth<'a>(
        &'a mut self,
        bandwidth: f32,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn set_center_frequency<'a>(
        &'a mut self,
        center_frequency: f32,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn shutdown<'a>(&'a mut self) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
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

    async fn try_open(&self, rtl2832u: &Rtl2832u) -> Result<Option<Self::Tuner>, Self::Error> {
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
        rtl2832u: &'a Rtl2832u,
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
        rtl2832u: &'a Rtl2832u,
    ) -> Pin<Box<dyn Future<Output = Result<Option<AnyTuner>, AnyTunerError>> + Send + 'a>> {
        Box::pin(
            self.try_open(rtl2832u)
                .map_ok(|tuner_opt| tuner_opt.map(AnyTuner::new))
                .map_err(AnyTunerError::new),
        )
    }
}

pub struct AnyTunerProbe(Box<dyn AnyTunerProbeTrait>);

impl Default for AnyTunerProbe {
    fn default() -> Self {
        Self::new(FallbackTunerProbe)
    }
}

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
        rtl2832u: &'a Rtl2832u,
    ) -> impl Future<Output = Result<Option<Self::Tuner>, Self::Error>> + Send + 'a {
        self.0.any_try_open(rtl2832u)
    }
}

trait AnyTunerTrait: Debug + Send + Sync + 'static {
    fn name(&self) -> &str;

    fn set_bandwidth<'a>(
        &'a mut self,
        bandwidth: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>>;

    fn set_center_frequency<'a>(
        &'a mut self,
        center_frequency: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>>;

    fn shutdown<'a>(
        &'a mut self,
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
        bandwidth: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>> {
        Box::pin(Tuner::set_bandwidth(self, bandwidth).map_err(AnyTunerError::new))
    }

    fn set_center_frequency<'a>(
        &'a mut self,
        center_frequency: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>> {
        Box::pin(Tuner::set_center_frequency(self, center_frequency).map_err(AnyTunerError::new))
    }

    fn shutdown<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyTunerError>> + Send + 'a>> {
        Box::pin(Tuner::shutdown(self).map_err(AnyTunerError::new))
    }
}

pub struct AnyTuner(Box<dyn AnyTunerTrait>);

impl AnyTuner {
    pub fn new(tuner: impl Tuner) -> Self {
        Self(Box::new(tuner))
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

    fn set_bandwidth(&mut self, bandwidth: f32) -> impl Future<Output = Result<(), Self::Error>> {
        self.0.set_bandwidth(bandwidth)
    }

    fn set_center_frequency<'a>(
        &'a mut self,
        center_frequency: f32,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.0.set_center_frequency(center_frequency)
    }

    fn shutdown(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.0.shutdown()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NullTuner;

impl Tuner for NullTuner {
    type Error = Infallible;

    fn name(&self) -> &str {
        "null"
    }

    async fn set_bandwidth(&mut self, bandwidth: f32) -> Result<(), Self::Error> {
        let _ = bandwidth;
        Ok(())
    }

    async fn set_center_frequency<'a>(
        &'a mut self,
        center_frequency: f32,
    ) -> Result<(), Self::Error> {
        let _ = center_frequency;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl TunerProbe for NullTuner {
    type Error = Infallible;
    type Tuner = Self;

    fn try_open<'a>(
        &'a self,
        rtl2832u: &'a Rtl2832u,
    ) -> impl Future<Output = Result<Option<Self::Tuner>, Self::Error>> + Send + 'a {
        let _ = rtl2832u;
        std::future::ready(Ok(Some(Self)))
    }
}
