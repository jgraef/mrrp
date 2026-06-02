use crate::{
    enumerate::DeviceInfo,
    rtl2832u::Rtl2832u,
    tuner::{
        Tuner,
        TunerError,
        TunerProbe,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // todo
}

impl TunerError for Error {}

#[derive(Clone, Debug)]
pub struct R82xxProbe;

impl TunerProbe for R82xxProbe {
    type Error = Error;
    type Tuner = R82xx;

    async fn try_open(
        &self,
        rtl2832u: &mut Rtl2832u,
        device_info: &DeviceInfo,
    ) -> Result<Option<Self::Tuner>, Self::Error> {
        todo!();
    }
}

#[derive(Debug)]
pub struct R82xx {
    // todo
}

impl Tuner for R82xx {
    type Error = Error;
}
