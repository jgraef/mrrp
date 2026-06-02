use crate::{
    rtl2832u::{
        self,
        Rtl2832u,
        i2c::{
            I2cAddress,
            I2cReadProbe,
            I2cRegister,
        },
    },
    tuner::{
        Tuner,
        TunerError,
        TunerProbe,
    },
};

pub const I2C_PROBE: I2cReadProbe = I2cReadProbe {
    register: I2cRegister(0x00),
    expected_value: 0x69,
};

pub const R820T_I2C_ADDR: I2cAddress = I2cAddress::from_left_aligned(0x34);
pub const R828D_I2C_ADDR: I2cAddress = I2cAddress::from_left_aligned(0x74);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Rtl2823u(#[from] rtl2832u::Error),
    // todo
}

impl TunerError for Error {}

#[derive(Clone, Debug)]
pub struct R82xxProbe;

impl TunerProbe for R82xxProbe {
    type Error = Error;
    type Tuner = R82xx;

    async fn try_open(&self, rtl2832u: &mut Rtl2832u) -> Result<Option<Self::Tuner>, Self::Error> {
        Ok(probe(rtl2832u).await?.then(|| R82xx {}))
    }
}

async fn probe(rtl2832u: &mut Rtl2832u) -> Result<bool, Error> {
    if I2C_PROBE.probe(rtl2832u, R820T_I2C_ADDR).await? {
        tracing::debug!("R820T found");
        return Ok(true);
    }

    if I2C_PROBE.probe(rtl2832u, R820T_I2C_ADDR).await? {
        tracing::debug!("R828D found");

        return Ok(true);
    }

    Ok(false)
}

#[derive(Debug)]
pub struct R82xx {
    // todo
}

impl Tuner for R82xx {
    type Error = Error;
}
