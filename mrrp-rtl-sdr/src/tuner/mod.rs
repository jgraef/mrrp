pub mod r82xx;

use std::{
    fmt::Debug,
    pin::Pin,
};

use crate::{
    enumerate::DeviceInfo,
    rtl2832u::Rtl2832u,
};

pub trait TunerError: std::error::Error + Send + Sync + Sized + 'static {}

pub trait TunerProbe: Clone + Debug + Sized + Send + 'static {
    type Error: TunerError;
    type Tuner: Tuner;

    fn try_open(
        &self,
        rtl2832u: &mut Rtl2832u,
        device_info: &DeviceInfo,
    ) -> impl Future<Output = Result<Option<Self::Tuner>, Self::Error>> + Send;
}

pub trait Tuner: Debug + Sized + Send + 'static {
    type Error: TunerError;
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
pub struct BultinTunerProbe;

impl TunerProbe for BultinTunerProbe {
    type Error = AnyTunerError;
    type Tuner = AnyTuner;

    async fn try_open(
        &self,
        rtl2832u: &mut Rtl2832u,
        device_info: &DeviceInfo,
    ) -> Result<Option<Self::Tuner>, Self::Error> {
        macro_rules! probe {
                {$($probe:expr,)*} => {
                    $(
                        if let Some(tuner) = $probe
                            .try_open(rtl2832u, device_info)
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

trait AnyTunerProbeTrait: Debug + Send + 'static {
    fn any_clone(&self) -> AnyTunerProbe;

    fn any_try_open(
        &self,
        rtl2832u: &mut Rtl2832u,
        device_info: &DeviceInfo,
    ) -> Pin<Box<dyn Future<Output = Result<Option<AnyTuner>, AnyTunerError>> + Send>>;
}

impl<T> AnyTunerProbeTrait for T
where
    T: TunerProbe,
{
    fn any_clone(&self) -> AnyTunerProbe {
        AnyTunerProbe::new(self.clone())
    }

    fn any_try_open(
        &self,
        rtl2832u: &mut Rtl2832u,
        device_info: &DeviceInfo,
    ) -> Pin<Box<dyn Future<Output = Result<Option<AnyTuner>, AnyTunerError>> + Send>> {
        todo!()
    }
}

pub struct AnyTunerProbe(Box<dyn AnyTunerProbeTrait>);

impl Default for AnyTunerProbe {
    fn default() -> Self {
        Self::new(BultinTunerProbe)
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

    fn try_open(
        &self,
        rtl2832u: &mut Rtl2832u,
        device_info: &DeviceInfo,
    ) -> impl Future<Output = Result<Option<Self::Tuner>, Self::Error>> + Send {
        self.0.any_try_open(rtl2832u, device_info)
    }
}

trait AnyTunerTrait: Debug + Send + 'static {
    // todo
}

impl<T> AnyTunerTrait for T
where
    T: Tuner,
{
    // todo
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
}
