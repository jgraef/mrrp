use std::ops::Deref;

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum InvalidFilter {
    #[error("FIR Filter coefficient out of range at index {index}")]
    CoefficientOutOfRange { index: usize },

    #[error("FIR Filter has incorrect length of {length} (instead of 16)")]
    InvalidLength { length: usize },
}

/// Linux uses
/// `\xca\xdc\xd7\xd8\xe0\xf2\x0e\x35\x06\x50\x9c\x0d\x71\x11\x14\x71\x74\x19\
/// x41\xa5`.
///
/// librtlsdr generates the bytes from proper filter coefficients:
///
/// ```c
/// /*
///  * FIR coefficients.
///  *
///  * The filter is running at XTal frequency. It is symmetric filter with 32
///  * coefficients. Only first 16 coefficients are specified, the other 16
///  * use the same values but in reversed order. The first coefficient in
///  * the array is the outer one, the last, the last is the inner one.
///  * First 8 coefficients are 8 bit signed integers, the next 8 coefficients
///  * are 12 bit signed integers. All coefficients have the same weight.
///  *
///  * Default FIR coefficients used for DAB/FM by the Windows driver,
///  * the DVB driver uses different ones
///  */
/// static const int fir_default[16] = {
/// 	-54, -36, -41, -40, -32, -14, 14, 53,	/* 8 bit signed */
/// 	101, 156, 215, 273, 327, 372, 404, 421	/* 12 bit signed */
/// };
/// ```
///
/// This is what it writes to memory (starting at 0x1c):
///
/// ```plain
/// │00000010│                         ┊             ca dc d7 d8 │        ┊    ××××│
/// │00000020│ e0 f2 0e 35 06 50 9c 0d ┊ 71 11 14 71 74 19 41 a5 │××•5•P×_┊q••qt•A×│
/// ```
///
/// So they both use the same filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirFilter {
    coefficients: [i16; 16],
}

impl FirFilter {
    pub const DEFAULT: Self = Self {
        coefficients: [
            -54, -36, -41, -40, -32, -14, 14, 53, 101, 156, 215, 273, 327, 372, 404, 421,
        ],
    };

    pub fn decode(buffer: &[u8; 20]) -> Self {
        let mut coefficients = [0; 16];

        for i in 0..8 {
            coefficients[i] = buffer[i].cast_signed().into();
        }

        for i in 0..4 {
            let x = u16::from(buffer[i * 3 + 8]);
            let y = u16::from(buffer[i * 3 + 9]);
            let z = u16::from(buffer[i * 3 + 10]);

            let mut a = (x << 4) | (y >> 4);
            let mut b = ((y & 0xf) << 8) | z;

            // sign-extend
            if a & 0x800 != 0 {
                a |= 0xf00;
            }
            if b & 0x800 != 0 {
                b |= 0xf00;
            }

            coefficients[i * 2 + 8] = a.cast_signed();
            coefficients[i * 2 + 9] = b.cast_signed();
        }

        Self { coefficients }
    }

    pub fn encode(&self, buffer: &mut [u8; 20]) {
        for i in 0..8 {
            buffer[i] = i8::try_from(self.coefficients[i]).unwrap().cast_unsigned();
        }

        // each iteration puts 2 i12 into 3 u8
        // input   fedcba987654 3210 fedcba98 76543210
        //         ----xxxxxxxx xxxx ----yyyy yyyyyyyy
        // output      76543210 7654     3210 76543210
        for i in 0..4 {
            let x = self.coefficients[i * 2 + 8].cast_unsigned();
            let y = self.coefficients[i * 2 + 9].cast_unsigned();

            buffer[i * 3 + 8] = (x >> 4).try_into().unwrap();
            buffer[i * 3 + 9] = (((x & 0x00f) << 4) | (y >> 8)).try_into().unwrap();
            buffer[i * 3 + 10] = (y & 0x0ff).try_into().unwrap();
        }
    }
}

impl Default for FirFilter {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Deref for FirFilter {
    type Target = [i16];

    fn deref(&self) -> &Self::Target {
        &self.coefficients
    }
}

impl AsRef<[i16]> for FirFilter {
    fn as_ref(&self) -> &[i16] {
        &self.coefficients
    }
}

impl TryFrom<[i16; 16]> for FirFilter {
    type Error = InvalidFilter;

    fn try_from(value: [i16; 16]) -> Result<Self, Self::Error> {
        for i in 0..8 {
            if i8::try_from(value[i]).is_err() {
                return Err(InvalidFilter::CoefficientOutOfRange { index: i });
            }
        }

        for i in 0..4 {
            let x = value[i * 2 + 8].cast_unsigned();
            let y = value[i * 2 + 9].cast_unsigned();

            // the upper 4 bits must either be 0 or f depending on the sign of the i12.
            // we check if these bits correspond to the msb of the i12.
            if x & 0xf80 != 0xf8 && x & 0xf80 != 0 {
                return Err(InvalidFilter::CoefficientOutOfRange { index: i * 2 });
            }
            if y & 0xf80 != 0xf8 && y & 0xf80 != 0 {
                return Err(InvalidFilter::CoefficientOutOfRange { index: i * 2 + 1 });
            }
        }

        Ok(Self {
            coefficients: value,
        })
    }
}

impl TryFrom<&[i16]> for FirFilter {
    type Error = InvalidFilter;

    fn try_from(value: &[i16]) -> Result<Self, Self::Error> {
        <[i16; 16]>::try_from(value)
            .map_err(|_| {
                InvalidFilter::InvalidLength {
                    length: value.len(),
                }
            })?
            .try_into()
    }
}
