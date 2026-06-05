use bytes::{
    Buf,
    BufMut,
    BytesMut,
};
use tokio_util::codec::{
    Decoder,
    Encoder,
};

use crate::{
    DongleInfo,
    TunerType,
};

/// Header length in bytes.
///
/// This consists of 4 bytes [`MAGIC`] and 8 bytes [`DongleInfo`].
pub const HEADER_LENGTH: usize = 12;

/// Length of a command in bytes
///
/// 1 byte for the command opcode, 4 bytes for the arguments.
pub const COMMAND_LENGTH: usize = 5;

/// Magic value sent by server to identify the protocol.
pub const MAGIC: &'static [u8; 4] = b"RTL0";

/// Tuner gain mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TunerGainMode {
    /// Tuner gain is set manually
    Manual,
    /// Tuner gain is set automatically by the tuner.
    Auto,
}

/// Direct sampling mode
///
/// Direct sampling is not yet supported by [`RtlSdr`], but it can be used with
/// [`rtl_tcp`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DirectSamplingMode {
    /// Direct sampling of I branch
    I,
    /// Direct sampling of Q branch
    Q,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub dongle_info: DongleInfo,
}

impl Header {
    pub fn decode<B>(buffer: &mut B) -> Result<Self, InvalidHeader>
    where
        B: Buf,
    {
        let mut magic_buf = [0; 4];

        buffer.copy_to_slice(&mut magic_buf);
        if &magic_buf != MAGIC {
            return Err(InvalidHeader::Magic { data: magic_buf });
        }

        let tuner_type = TunerType(buffer.get_u32());
        let tuner_gain_count = buffer.get_u32();

        Ok(Self {
            dongle_info: DongleInfo {
                tuner_type,
                tuner_gain_count,
            },
        })
    }

    pub fn encode<B>(&self, buffer: &mut B)
    where
        B: BufMut,
    {
        buffer.put(&MAGIC[..]);
        buffer.put_u32(self.dongle_info.tuner_type.0);
        buffer.put_u32(self.dongle_info.tuner_gain_count);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidHeader {
    #[error("Invalid header magic: {data:?}")]
    Magic { data: [u8; 4] },
}

/// Commands that can be send to the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    SetCenterFrequency { frequency: u32 },
    SetSampleRate { sample_rate: u32 },
    SetTunerGainMode { mode: TunerGainMode },
    SetTunerGain { gain: i32 },
    SetFrequencyCorrection { ppm: i32 },
    SetTunerIfGain { stage: i16, gain: i16 },
    SetTestMode { enable: bool },
    SetAgcMode { enable: bool },
    SetDirectSampling { mode: Option<DirectSamplingMode> },
    SetOffsetTuning { enable: bool },
    SetRtlXtal { frequency: u32 },
    SetTunerXtal { frequency: u32 },
    SetTunerGainIndex { index: u32 },
    SetBiasT { enable: bool },
}

impl Command {
    pub fn decode<B>(buffer: &mut B) -> Result<Self, InvalidCommand>
    where
        B: Buf,
    {
        match buffer.get_u8() {
            0x01 => {
                Ok(Self::SetCenterFrequency {
                    frequency: buffer.get_u32(),
                })
            }
            0x02 => {
                Ok(Self::SetSampleRate {
                    sample_rate: buffer.get_u32(),
                })
            }
            0x03 => {
                Ok(Self::SetTunerGainMode {
                    mode: if buffer.get_u32() == 0 {
                        TunerGainMode::Auto
                    }
                    else {
                        TunerGainMode::Manual
                    },
                })
            }
            0x04 => {
                Ok(Self::SetTunerGain {
                    gain: buffer.get_i32(),
                })
            }
            0x05 => {
                Ok(Self::SetFrequencyCorrection {
                    ppm: buffer.get_i32(),
                })
            }
            0x06 => {
                Ok(Self::SetTunerIfGain {
                    stage: buffer.get_i16(),
                    gain: buffer.get_i16(),
                })
            }
            0x07 => {
                Ok(Self::SetTestMode {
                    enable: buffer.get_u32() != 0,
                })
            }
            0x08 => {
                Ok(Self::SetAgcMode {
                    enable: buffer.get_u32() != 0,
                })
            }
            0x09 => {
                Ok(Self::SetDirectSampling {
                    mode: match buffer.get_u32() {
                        1 => Some(DirectSamplingMode::I),
                        2 => Some(DirectSamplingMode::Q),
                        _ => None,
                    },
                })
            }
            0x0a => {
                Ok(Self::SetOffsetTuning {
                    enable: buffer.get_u32() != 0,
                })
            }
            0x0b => {
                Ok(Self::SetRtlXtal {
                    frequency: buffer.get_u32(),
                })
            }
            0x0c => {
                Ok(Self::SetTunerXtal {
                    frequency: buffer.get_u32(),
                })
            }
            0x0d => {
                Ok(Self::SetTunerGainIndex {
                    index: buffer.get_u32(),
                })
            }
            0x0e => {
                Ok(Self::SetBiasT {
                    enable: buffer.get_u32() != 0,
                })
            }
            command => {
                let mut arguments = [0; 4];
                buffer.copy_to_slice(&mut arguments);
                Err(InvalidCommand { command, arguments })
            }
        }
    }

    pub fn encode<B>(&self, buffer: &mut B)
    where
        B: BufMut,
    {
        match self {
            Self::SetCenterFrequency { frequency } => {
                buffer.put_u8(0x01);
                buffer.put_u32(*frequency);
            }
            Self::SetSampleRate { sample_rate } => {
                buffer.put_u8(0x02);
                buffer.put_u32(*sample_rate);
            }
            Self::SetTunerGainMode { mode } => {
                buffer.put_u8(0x03);
                buffer.put_u32(match mode {
                    TunerGainMode::Auto => 0,
                    TunerGainMode::Manual => 1,
                });
            }
            Self::SetTunerGain { gain } => {
                buffer.put_u8(0x04);
                buffer.put_i32(*gain);
            }
            Self::SetFrequencyCorrection { ppm } => {
                buffer.put_u8(0x05);
                buffer.put_i32(*ppm);
            }
            Self::SetTunerIfGain { stage, gain } => {
                buffer.put_u8(0x06);
                buffer.put_i16(*stage);
                buffer.put_i16(*gain);
            }
            Self::SetTestMode { enable } => {
                buffer.put_u8(0x07);
                buffer.put_u32(*enable as u32);
            }
            Self::SetAgcMode { enable } => {
                buffer.put_u8(0x08);
                buffer.put_u32(*enable as u32);
            }
            Self::SetDirectSampling { mode } => {
                buffer.put_u8(0x09);
                buffer.put_u32(match mode {
                    None => 0,
                    Some(DirectSamplingMode::I) => 1,
                    Some(DirectSamplingMode::Q) => 2,
                });
            }
            Self::SetOffsetTuning { enable } => {
                buffer.put_u8(0x0a);
                buffer.put_u32(*enable as u32);
            }
            Self::SetRtlXtal { frequency } => {
                buffer.put_u8(0x0b);
                buffer.put_u32(*frequency);
            }
            Self::SetTunerXtal { frequency } => {
                buffer.put_u8(0x0c);
                buffer.put_u32(*frequency);
            }
            Self::SetTunerGainIndex { index } => {
                buffer.put_u8(0x0d);
                buffer.put_u32(*index);
            }
            Self::SetBiasT { enable } => {
                buffer.put_u8(0x0e);
                buffer.put_u32(*enable as u32);
            }
        }
    }
}

/// Error for when an invalid command is received.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, thiserror::Error)]
#[error("Invalid rtl_tcp command: 0x{command:02} (arguments: {arguments:?})")]
pub struct InvalidCommand {
    pub command: u8,
    pub arguments: [u8; 4],
}

#[derive(Debug, thiserror::Error)]
pub enum DecoderError<P> {
    Protocol(#[source] P),

    Io(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CommandCodec;

impl Decoder for CommandCodec {
    type Item = Command;
    type Error = DecoderError<InvalidCommand>;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.remaining() >= COMMAND_LENGTH {
            Command::decode(src)
                .map(Some)
                .map_err(DecoderError::Protocol)
        }
        else {
            Ok(None)
        }
    }
}

impl Encoder<Command> for CommandCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: Command, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.encode(dst);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HeaderCodec;

impl Decoder for HeaderCodec {
    type Item = Header;
    type Error = DecoderError<InvalidHeader>;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.remaining() >= HEADER_LENGTH {
            Header::decode(src)
                .map(Some)
                .map_err(DecoderError::Protocol)
        }
        else {
            Ok(None)
        }
    }
}

impl Encoder<Header> for HeaderCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: Header, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.encode(dst);
        Ok(())
    }
}
