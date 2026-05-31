use bitfield::bitfield;
use nusb::transfer::{
    ControlIn,
    ControlOut,
    ControlType,
    Recipient,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Register {
    Demod { page: u8, address: u16 },
    Usb { address: u16 },
    System { address: u16 },
    Tuner { address: u16 },
    Rom { address: u16 },
    I2c { address: u16 },
}

impl Register {
    pub fn w_value(&self) -> u16 {
        match self {
            Register::Demod { page: _, address } => *address,
            Register::Usb { address } => *address,
            Register::System { address } => *address,
            Register::Tuner { address } => *address,
            Register::Rom { address } => *address,
            Register::I2c { address } => *address,
        }
    }

    pub fn w_index(&self, write: bool) -> u16 {
        let mut w_index = match self {
            Register::Demod { page, address: _ } => {
                assert!(*page <= 4);
                u16::from(*page)
            }
            Register::Usb { address: _ } => 0x0100,
            Register::System { address: _ } => 0x0200,
            Register::Tuner { address: _ } => 0x0300,
            Register::Rom { address: _ } => 0x0400,
            Register::I2c { address: _ } => 0x0600,
        };

        if write {
            w_index |= 0x10;
        }

        w_index
    }

    pub fn control_in(&self, length: u16) -> ControlIn {
        ControlIn {
            // bmRequestType, vendor command
            control_type: ControlType::Vendor,
            // bmRequestType, endpoint
            recipient: Recipient::Endpoint,
            // bmRequest
            request: 0,
            // wValue
            value: self.w_value(),
            // wIndex
            index: self.w_index(false),
            // wLength
            length,
        }
    }

    pub fn control_out<'a>(&self, data: &'a [u8]) -> ControlOut<'a> {
        ControlOut {
            // bmRequestType, vendor command
            control_type: ControlType::Vendor,
            // bmRequestType, endpoint
            recipient: Recipient::Endpoint,
            // bmRequest
            request: 0,
            // wValue
            value: self.w_value(),
            // wIndex
            index: self.w_index(true),
            // wLength implicit
            data,
        }
    }

    pub const fn demod(page: u8, address: u16) -> Self {
        Self::Demod { page, address }
    }

    pub const fn usb(address: u16) -> Self {
        Self::Usb { address }
    }
}

pub trait RegisterAddress {
    const ADDRESS: Register;
}

/// Macro that lets us define registers more easily
macro_rules! registers {
    {
        $(
            $(#[$attrs:meta])* $name:ident = $block:ident $args:tt $fields:tt;
        )*
    } => {
        $(
            registers!(@generate_code(($($attrs)*), $name, registers!(@parse_address($block, $args)), $fields));
        )*

        pub const ALL: &[Register] = &[$($name::ADDRESS),*];
    };
    (@parse_address(demod, ($page:expr, $address:expr))) => {
        Register::demod($page, $address)
    };
    (@parse_address($block:ident, ($address:expr))) => {
        Register::$block($address)
    };
    (@generate_code(($($attrs:meta)*), $name:ident, $address:expr, {$($fields:tt)*})) => {
        bitfield! {
            $(#[$attrs])*
            pub struct $name(u32);
            impl Debug;
            impl new;
            $($fields)*
        }

        #[automatically_derived]
        impl RegisterAddress for $name {
            const ADDRESS: Register = $address;
        }

        #[automatically_derived]
        impl From<u32> for $name {
            #[inline(always)]
            fn from(value: u32) -> Self {
                Self(value)
            }
        }

        #[automatically_derived]
        impl From<$name> for u32 {
            #[inline(always)]
            fn from(value: $name) -> Self {
                value.0
            }
        }

        #[automatically_derived]
        impl Default for $name {
            #[inline(always)]
            fn default() -> Self {
                Self(0)
            }
        }
    }
}

macro_rules! make_enum {
    {pub enum $name:ident($int:ty) $body:tt} => {
        make_enum!(@generate_code((pub), $name, $int, $body));
    };
    {enum $name:ident($int:ty) $body:tt} => {
        make_enum!(@generate_code((), $name, $int, $body));
    };
    (@generate_code(($($vis:tt)?), $name:ident, $int:ty, {$($variant:ident = $value:expr,)*})) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $($vis)? enum $name {
            $($variant,)*
        }

        #[automatically_derived]
        impl From<$name> for $int {
            #[inline]
            fn from(value: $name) -> Self {
                match value {
                    $($name::$variant => $value,)*
                }
            }
        }

        #[automatically_derived]
        impl TryFrom<$int> for $name {
            type Error = $int;

            #[inline]
            fn try_from(value: $int) -> Result<Self, Self::Error> {
                match value {
                    $($value => Ok(Self::$variant),)*
                    _ => Err(value),
                }
            }
        }
    };
}

pub mod usb {
    use super::*;

    registers! {
        SystemControl = usb(0x2000) {
            pub bool, sie_reset, set_sie_reset: 10;
            pub bool, full_packet_mode, set_full_packet_mode: 3;
            pub bool, dma_enable, set_dma_enable: 0;
        };
        EndpointAConfig = usb(0x2144) {
            pub u8, isochronous_mode, set_isochronous_mode: 9, 8;
            pub bool, endpoint_enable, set_endpoint_enable: 7;
            pub u8, from try_into EndpointTransferType, endpoint_transfer_type, set_endpoint_transfer_type: 6, 5;
            pub u8, from try_into EndpointTransferDirection, endpoint_transfer_direction, set_endpoint_transfer_direction: 4, 4;
            pub u8, endpoint_number, set_endpoint_number: 3, 0;
        };
        EndpointAControl = usb(0x2148) {
            pub bool, fifo_reset, set_fifo_reset: 9;
            pub bool, fifo_flush, set_fifo_flush: 5;
            pub bool, stall_endpoint, set_stall_endpoint: 4;
            pub bool, fifo_valid, set_fifo_valid: 0;
        };

        /// Configures the max packet size.
        ///
        /// Valid values are `0..=1024` (10 bits)
        EndpointAMaxPacketSize = usb(0x2158) {
            pub u16, max_packet_size, set_max_packet_size: 10, 0;
        };

        /// Configures FIFO
        ///
        /// `fifo_size` can be `0..=8` (3 bits)
        EndpointAFifoConfig = usb(0x2160) {
            pub u8, block_drop_counter, set_block_drop_counter: 31, 24;
            pub u8, fifo_size, set_fifo_size: 3, 0;
        };
    }

    make_enum! {
        pub enum EndpointTransferType(u8) {
            Control = 0b00,
            Isochrronous = 0b01,
            Bulk = 0b10,
            Interrupt = 0b11,
        }
    }

    make_enum! {
        pub enum EndpointTransferDirection(u8) {
            HostToDevice = 0,
            DeviceToHost = 1,
        }
    }
}

pub mod adc {
    pub use super::*;

    registers! {
        AdcEnable = demod(0, 0x08) {
            pub bool, enable_q, set_enable_q: 6;
            pub bool, enable_i, set_enable_i: 7;
        };
    }
}
