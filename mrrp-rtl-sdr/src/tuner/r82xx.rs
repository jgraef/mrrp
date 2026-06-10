//! Rafael R820T and R828D tuners
//!
//! ![R828D chip on a Blog V4](https://cdn.eenewseurope.com/wp-content/uploads/2023/08/Screen-Shot-2023-08-23-at-09.25.22.png)
//!
//! [![R828D chip on a PCB](https://blogger.googleusercontent.com/img/b/R29vZ2xl/AVvXsEg7TxRzyPFL3pJA8EXOVpqcvuo0f1Bl7ZU1waPUvN32uWn94AepDHJUNQXrNwTZdw1bNTzf6U4y4nIqCZsn4pmtIVGmk4prXKTqdZVb_SE85JV_jlJE8APbDoLEUSop289DHHZU69JP2mQ/s1600/1024-22.jpg)]((http://blog.palosaari.fi/2013/10/naked-hardware-14-dvb-t2-usb-tv-stick.html))
//!
//! [RTL-SDR V3 Teardown and Analysis](https://www.onelectrontech.com/rtl-sdr-v3-teardown-and-analysis/)
//!
//! [This website](https://www.jotrin.com/product/parts/R828D) has some limited info about the R828D, mentioning dual LNA. Maybe that's what the `cable_X_in` bits switch?
//!
//! And [this post](https://www.rtl-sdr.com/new-rtl-sdr-tuner-chip-r828d/) even quotes a Reddit comment from a driver author, mentioning the 3 inputs "Air-In, Cable1, Cable2".
//!
//! [Registers of R82xx](http://www.erlendervik.no/r820-reg.ods) (this is the "spreadsheet" we're referrring to)
//!
//! ![R828D pinout](https://www.erlendervik.no/r828d.png)

use std::{
    fmt::Debug,
    ops::{
        Index,
        IndexMut,
        Range,
    },
};

use paste::paste;
use pretty_hex::PrettyHex;

use crate::{
    rtl2832u::{
        self,
        Rtl2832u,
        i2c::{
            I2cAddress,
            I2cDevice,
        },
    },
    tuner::{
        Tuner,
        TunerError,
        TunerProbe,
    },
};

// todo: remove this. the IF frequency depends on the bandwidth
pub const DEFAULT_IF_FREQUENCY: u32 = 3570000;
pub const CRYSTAL_FREQ: u32 = 16000000;

pub const NUM_REGISTERS: u8 = 0x20;

/// Initial values, starting from 0x05
#[rustfmt::skip]
pub const INITIAL: &[u8] = &[
          0x83, 0x30, 0x75, // 0x05 ..= 0x07
    0xc0, 0x40, 0xd6, 0x6c, // 0x08 ..= 0x0b
    0xf5, 0x63, 0x75, 0x68, // 0x0c ..= 0x0f
    0x6c, 0x83, 0x80, 0x00, // 0x10 ..= 0x13
    0x0f, 0x00, 0xc0, 0x30, // 0x14 ..= 0x17
    0x48, 0xcc, 0x60, 0x00, // 0x18 ..= 0x1b
    0x54, 0xae, 0x4a, 0xc0, // 0x1c ..= 0x1f
];

pub const MAX_I2C_MESSAGE_LENGTH: u8 = 0x08;
pub const VERSION_VALUE: u8 = 0x31;

#[derive(Clone, Copy, Debug)]
pub struct FilterSetting {
    pub min_bandwidth: f32,
    pub max_bandwidth: f32,
    pub low_q: bool,
    pub bw_1_7mhz: bool,
    pub filt_bw: u8,
    pub hpf: u8,
    pub center_frequency: f32,
}

pub const FILTER_SETTINGS: &[FilterSetting] = &[
    FilterSetting {
        min_bandwidth: 7000000.0,
        max_bandwidth: 8000000.0,
        low_q: true,
        bw_1_7mhz: false,
        filt_bw: 0,
        hpf: 11,
        center_frequency: 4570000.0,
    },
    FilterSetting {
        min_bandwidth: 6000000.0,
        max_bandwidth: 7000000.0,
        low_q: true,
        bw_1_7mhz: false,
        filt_bw: 1,
        hpf: 10,
        center_frequency: 4570000.0,
    },
    FilterSetting {
        min_bandwidth: 2430000.0,
        max_bandwidth: 6000000.0,
        low_q: true,
        bw_1_7mhz: false,
        filt_bw: 3,
        hpf: 11,
        center_frequency: 3570000.0,
    },
    FilterSetting {
        min_bandwidth: 2050000.0,
        max_bandwidth: 2430000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 2,
        hpf: 15,
        center_frequency: 1640000.0,
    },
    FilterSetting {
        min_bandwidth: 1700000.0,
        max_bandwidth: 2050000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 1,
        hpf: 12,
        center_frequency: 1750000.0,
    },
    FilterSetting {
        min_bandwidth: 1600000.0,
        max_bandwidth: 1700000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 3,
        hpf: 15,
        center_frequency: 1450000.0,
    },
    FilterSetting {
        min_bandwidth: 1550000.0,
        max_bandwidth: 1600000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 3,
        hpf: 14,
        center_frequency: 1500000.0,
    },
    FilterSetting {
        min_bandwidth: 1450000.0,
        max_bandwidth: 1550000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 3,
        hpf: 13,
        center_frequency: 1525000.0,
    },
    FilterSetting {
        min_bandwidth: 1200000.0,
        max_bandwidth: 1450000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 3,
        hpf: 12,
        center_frequency: 1575000.0,
    },
    FilterSetting {
        min_bandwidth: 700000.0,
        max_bandwidth: 1200000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 3,
        hpf: 10,
        center_frequency: 1850000.0,
    },
    FilterSetting {
        min_bandwidth: 550000.0,
        max_bandwidth: 700000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 3,
        hpf: 9,
        center_frequency: 1950000.0,
    },
    FilterSetting {
        min_bandwidth: 450000.0,
        max_bandwidth: 550000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 3,
        hpf: 8,
        center_frequency: 2025000.0,
    },
    FilterSetting {
        min_bandwidth: 350000.0,
        max_bandwidth: 450000.0,
        low_q: false,
        bw_1_7mhz: true,
        filt_bw: 3,
        hpf: 7,
        center_frequency: 2075000.0,
    },
];

pub fn filter_setting_for_bandwidth(bandwidth: f32) -> &'static FilterSetting {
    FILTER_SETTINGS
        .iter()
        .find(|filter_setting| filter_setting.min_bandwidth < bandwidth)
        .unwrap_or_else(|| FILTER_SETTINGS.last().unwrap())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Model {
    R820T,
    R828D,
}

impl Model {
    pub const ALL: &[Self] = &[Self::R820T, Self::R828D];

    pub const fn name(&self) -> &'static str {
        match self {
            Model::R820T => "R820T",
            Model::R828D => "R828D",
        }
    }

    pub const fn i2c_address(&self) -> I2cAddress {
        match self {
            Model::R820T => I2cAddress::from_left_aligned(0x34),
            Model::R828D => I2cAddress::from_left_aligned(0x74),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Rtl2823u(#[from] rtl2832u::Error),

    #[error(transparent)]
    I2cDeviceBusy(#[from] rtl2832u::i2c::I2cDeviceBusy),
}

impl TunerError for Error {}

#[derive(Clone, Debug)]
pub struct R82xxProbe;

impl TunerProbe for R82xxProbe {
    type Error = Error;
    type Tuner = R82xx;

    async fn try_open(&self, rtl2832u: &Rtl2832u) -> Result<Option<Self::Tuner>, Self::Error> {
        for model in Model::ALL {
            tracing::debug!("probing for {}", model.name());

            let mut i2c_device = rtl2832u.try_open_i2c(model.i2c_address())?.with_repeater();

            // According to datasheet this is 0x96, but the chip sends data from LSB
            // to MSB, while the RTL2832U decodes it the other way.

            if let Ok(data) = i2c_device.read(1).await
                && data[0] == 0x69
            {
                tracing::debug!("{} found", model.name());

                let mut r82xx = R82xx::new(i2c_device, *model);

                let mut transaction = r82xx.begin_transaction();
                transaction.initialize().await?;
                transaction.commit().await?;

                return Ok(Some(r82xx));
            }
        }

        Ok(None)
    }
}

/// R82xx tuner
#[derive(Debug)]
pub struct R82xx {
    model: Model,
    i2c_device: I2cDevice,
    register_state: [u8; NUM_REGISTERS as usize],
    if_frequency: f32,
}

impl R82xx {
    pub fn new(i2c_device: I2cDevice, model: Model) -> Self {
        Self {
            model,
            i2c_device,
            register_state: Default::default(),
            if_frequency: DEFAULT_IF_FREQUENCY as f32,
        }
    }

    #[inline(always)]
    pub fn model(&self) -> Model {
        self.model
    }

    /// Begin a transaction that can read and write registers.
    ///
    /// This is mostly used to batch writes. You can flush writes with
    /// [`Transaction::flush`], or call [`Transaction::commit`] when
    /// you're done. The latter consumes the transaction, but also flushes
    /// all writes.
    pub fn begin_transaction<'a>(&'a mut self) -> Transaction<'a> {
        let registers = RegisterBuffer {
            state: self.register_state,
            modified: 0,
        };

        let if_frequency = self.if_frequency;

        Transaction {
            r82xx: self,
            registers,
            if_frequency,
            warn_on_uncomitted_drop: true,
        }
    }
}

#[derive(Debug)]
pub struct Transaction<'a> {
    r82xx: &'a mut R82xx,
    pub registers: RegisterBuffer,
    if_frequency: f32,
    pub warn_on_uncomitted_drop: bool,
}

impl<'a> Transaction<'a> {
    #[inline(always)]
    pub fn r82xx(&self) -> &R82xx {
        &self.r82xx
    }

    #[inline(always)]
    pub fn model(&self) -> Model {
        self.r82xx.model
    }

    /// Reads the first `n` registers from the device into the local cache.
    ///
    /// Always starts reading from register 0x00.
    ///
    /// The max message length of 0x08 is used by librtlsdr for R82xx. While
    /// testing we were able to read upto 0x10 bytes. We'll only ever need the
    /// first 5 bytes though.
    ///
    /// This doesn't read any registers that have been modified during this
    /// transaction.
    ///
    /// The R82xx sends bits in reversed order. This accounts for this and
    /// reverses the received bits.
    pub async fn read(&mut self, n: u8) -> Result<(), Error> {
        let data = self.r82xx.i2c_device.read(n.into()).await?;

        for i in 0..n {
            if !self.registers.is_modified(i) {
                // R82xx sends bytes with bits reversed.
                let value = data[usize::from(i)].reverse_bits();

                // Don't deref_mut into registers directly to avoid setting the dirty bit.
                self.registers.state[usize::from(i)] = value;

                // also write into backing storage
                self.r82xx.register_state[usize::from(i)] = value;
            }
        }

        tracing::debug!(registers = ?self.registers.state[..usize::from(n)], "read registers");

        Ok(())
    }

    pub async fn commit(mut self) -> Result<(), Error> {
        self.flush().await
    }

    /// Write out any dirty registers
    ///
    /// This will ever only write registers that are marked as dirty, but it'll
    /// try to do so in as few write commands as possible.
    pub async fn flush(&mut self) -> Result<(), Error> {
        tracing::debug!(
            "flushing registers\nmodified: {:?}\n{:?}",
            FormatRegisterBitMap(self.registers.modified),
            self.registers.state.hex_dump()
        );

        let mut run = None;
        let mut buf = [0u8; 0x20];

        let mut flush_run = async |run: Range<u8>| -> Result<(), Error> {
            buf[0] = run.start;
            buf[1..usize::from(run.end) - usize::from(run.start) + 1]
                .copy_from_slice(&self.registers[run.clone()]);

            let buf_len = 1 + run.end - run.start;
            let command = &buf[..buf_len.into()];

            tracing::debug!(?run, ?command, "write registers");

            self.r82xx.i2c_device.write(&command).await?;

            Ok(())
        };

        for i in 0..NUM_REGISTERS {
            if run
                .as_ref()
                .is_some_and(|run: &Range<u8>| run.end - run.start + 1 == MAX_I2C_MESSAGE_LENGTH)
            {
                flush_run(run.take().unwrap()).await?;
            }

            if self.registers.is_modified(i)
                && self.registers[i] != self.r82xx.register_state[usize::from(i)]
            {
                run.get_or_insert_with(|| i..i).end += 1;
            }
            else if let Some(run) = run.take() {
                flush_run(run).await?;
            }
        }

        if let Some(run) = run.take() {
            flush_run(run).await?;
        }

        // write registers back into backing state
        self.r82xx.register_state = self.registers.state;

        // clear all modified flags
        self.registers.modified = 0;

        Ok(())
    }

    /// Initializes the R82xx
    ///
    /// Since this needs to peform calibration, it needs to flush the
    /// transaction during initialization. But it will not flush the transaction
    /// when it's done initializing. The caller has to do this - so they might
    /// queue up more writes.
    pub async fn initialize(&mut self) -> Result<(), Error> {
        tracing::debug!(tuner = ?self.r82xx.model, "initializing");
        //tracing::debug!("initial state: {:#?}", self.state);

        //self.sync().await?;
        //tracing::debug!("synced state: {state:#?}");

        // TODO: do we want to remove this and instead explicitely initialize registers
        // via setters? we should also remove anything that is overwritten immediately
        // after. we would still have to keep this to initialize some bits that are
        // never changed. though librtlsdr uses this in a few places.
        self.registers[5..NUM_REGISTERS].copy_from_slice(&INITIAL);

        // the following initialization is derived from `r82xx_set_tv_standard`
        //
        // note: right now this doesn't do any async, so we could have this on the state
        // instead. but the librtlsdr code also calibrates the device, which would need
        // to flush.

        // initialize VGA gain
        // on, controlled by vagc pin
        self.registers.set_pwd_vga(true);
        self.registers.set_vga_mode(true);
        self.registers.set_vga_code(0);

        // VCO band
        // rc = r82xx_write_reg_mask(priv, 0x13, VER_NUM, 0x3f);
        self.registers.set_unk_vco_band(VERSION_VALUE);

        // for LT (loop-through) gain test?
        // only if not analog tv
        self.registers.set_pdet1_gain(0);

        // todo: calibration
        // here during calibration librtlsdr will call r82xx_set_pll, which will set
        // sel_div.

        // this also done in r82xx_set_pll, which we'll hardcode here for testing
        //
        // /* set VCO current = 100 */
        // /* rc = r82xx_write_reg_mask(priv, 0x12, 0x80, 0xe0); */
        // /* RTL-SDR Blog Modification: Set VCO current to MAX */
        // rc = r82xx_write_reg_mask(priv, 0x12, 0x06, 0xff);
        //
        self.registers.set_unk_vco_current(0b000);
        self.registers.set_unk_cp_offset(0b11);
        self.registers.set_s_i2c(0b10);
        self.registers.set_n_i2c(0b000100);
        self.registers.set_sdm_in(0x1c72); // 72 1c
        self.registers.set_pll_auto_clk(0b10);

        // todo: hard-coded for testing
        self.registers.set_sel_div(0b100);

        // filter bandwidth manual fine tune: widest
        //
        // librtlsdr falls back to 0b0000.
        //
        // we'll use 0b0101, since that's what rtl_tcp dumped when instrumented.
        let filt_code = 0b0101;
        self.registers.set_filt_code(filt_code);

        // unknown
        self.registers.set_unk_filt_q(true);

        // filter bandwidth: narrowest
        self.registers.set_filt_bw(0b11);

        // HPF corner control
        self.registers.set_hpf(0b1011);

        // set img_r?
        self.registers.set_unk_img_r(false);

        // set filter gain to 3 dB
        self.registers.set_filt_3db(true);

        // unknown
        self.registers.set_unk_v6mhz(true);

        // channel filter extension on
        self.registers.set_filter_ext(true);

        // librtlsdr comments this as "r30[5]:1 ext at lna max-1", but only sets the msb
        // of pdet_clk to 1, while the rest is still `0b_1010` from initialization.
        // this is now split from pdet_clk
        self.registers.set_ext_enable(true);

        // pwd loop-through off
        self.registers.set_pwd_lt(true);

        // loop-through attenuation on
        self.registers.set_unk_lt_att(false);

        // filter extension widest: off
        self.registers.set_unk_filt_ext_widest(false);

        // RF poly filter current
        // unknown, this might be minimum
        self.registers.set_unk_rf_poly_filter_current(0b11);

        // Enable RF filter power
        // librtlsdr doesn't do this explicitely here, but it's in the initialized
        // register bytes.
        self.registers.set_pwd_rffilt(true);

        // the following is from `r82xx_sysfreq_sel`

        // lna_top = 0xe5;		/* detect bw 3, lna top:4, predet top:2 */
        // rc = r82xx_write_reg_mask(priv, 0x1d, lna_top, 0xc7);
        // mask    = 0b1100_0111
        // lna_top = 0b1110_0101
        // ugh, what the hell are they doing here?
        self.registers.set_unk_detect_bw(3);
        self.registers.set_pdet1_gain(4);
        self.registers.set_pdet2_gain(5);

        // mixer_top = 0x14;	/* mixer top:14 , top-1, low-discharge */
        // mixer_top = 0x24;	/* mixer top:13 , top-1, low-discharge */
        // rc = r82xx_write_reg_mask(priv, 0x1c, mixer_top, 0xf8);
        // mask      = 0b1111_1000
        // mixer_top(0x14) = 0b0001_0100
        // mixer_top(0x24) = 0b0010_0100
        //                     GGGG TDso
        // g=pdet1_gain, t=lna_top_p1, d=discharge_mode, s=mixer_src, o=vco_out
        //
        // note the write mask, so discharge_mode, mixer_src and vco_out are not
        // changed.
        //
        // mixer_top is always 0x24, except DVBT freqiencies 506000000, 666000000,
        // 818000000
        self.registers.set_pdet3_gain(2);
        self.registers.set_unk_lna_top_p1(false);

        // lna_vth_l = 0x53;		/* lna vth 0.84	,  vtl 0.64 */ for ISDBT they use another
        // value rc = r82xx_write_reg(priv, 0x0d, lna_vth_l);
        //
        // => LNA_VTH_H = 0x05, 0b0101 => 0.873V - you'll get 0.84V with rounded step
        // => LNA_VTH_L = 0x03, 0b0011 => 0.660V - you'll get 0.64V with rounded step
        //
        // asserts are here to check if their rounding makes a difference.

        let lna_vth_h = voltage_to_lna_vth(0.84);
        //let lna_vth_h = voltage_to_lna_vth(0.873);
        assert_eq!(lna_vth_h, 0x05);
        self.registers.set_lnavth_h(lna_vth_h);

        let lna_vth_l = voltage_to_lna_vth(0.64);
        //let lna_vth_l = voltage_to_lna_vth(0.660);
        assert_eq!(lna_vth_l, 0x03);
        self.registers.set_lnavth_l(lna_vth_l);

        // mixer_vth_l = 0x75;		/* mixer vth 1.04, vtl 0.84 */
        // rc = r82xx_write_reg(priv, 0x0e, mixer_vth_l);
        //
        // => MIX_VTH_H = 0x07, 0b0111 => 1.086 - you'll get 1.04 with rounded step
        // => MIX_VTH_L = 0x05, 0b0101 => 0.873V - you'll get 0.84V with rounded step
        //
        let mixer_vth_h = voltage_to_lna_vth(1.04);
        assert_eq!(mixer_vth_h, 0x07);
        self.registers.set_mixvth_h(mixer_vth_h);

        let mixer_vth_l = voltage_to_lna_vth(0.84);
        assert_eq!(mixer_vth_l, 0x05);
        self.registers.set_mixvth_l(mixer_vth_l);

        // air_cable1_in = 0
        // /* Air-IN only for Astrometa */
        // rc = r82xx_write_reg_mask(priv, 0x05, air_cable1_in, 0x60);
        // mask = 0b0110_0000
        //
        // so set PWD_LNA1 = 0(on). this is also labelled `air_in` in librtlsdr.
        // but we think it's just that they only turn the LNA on for the air_in, and
        // `cable_1_in`, or `cable_2_in` switch to the other inputs on the R828D.
        //
        // and bit 6 to 0, but it is initialized as that and is fixed to that in the
        // datasheet. this seems to be another input `cable_1_in`
        //
        // note that the R820T only has one RF_in. The R828D has 3 inputs: air_in
        // (RF_in), cable_1_in, cable_2_in
        //
        // todo: merge this into a `select_input` method on `R82xxState`.
        //self.registers.set_pwd_lna1(false); // LNA power on
        //self.registers.set_unk_cable_1_in(false); // Cable 1 input off
        //
        // cable2_in = 0x00;
        // rc = r82xx_write_reg_mask(priv, 0x06, cable2_in, 0x08);
        //self.registers.set_unk_cable_2_in(false); // Cable 2 input off

        // instead of the above nonsense, we'll do this.
        // but we'll keep the above for a while for reference.

        // power on LNA
        // todo: ideally expose this via method on state too.
        self.registers.set_pwd_lna1(false);
        self.select_rf_input(RfInput::Air);

        // what is this? librtlsdr only says that 0b111 means auto
        //
        // there's a CP pin - PLL charge pump
        self.registers.set_unk_cp_cur(0b111);

        // set div_buf_cur
        //
        // RTL-SDR Blog Hack. Improve L-band performance by setting PLL drop out to 2.0v
        // div_buf_cur = 0xa0;
        // rc = r82xx_write_reg_mask(priv, 0x17, div_buf_cur, 0x30);
        //
        // this write is masked with 0x30, so it just sets 0b10.
        self.registers.set_unk_div_buf_cur(0b10);

        // setup LNA

        // this actually sets it to the highest setting
        // /* LNA TOP: lowest */
        // rc = r82xx_write_reg_mask(priv, 0x1d, 0, 0x38);
        // mask = 0b0011_1000
        //
        // this was previously set to 4. so what should it be?
        self.registers.set_pdet1_gain(0);

        // /* 0: normal mode */
        // rc = r82xx_write_reg_mask(priv, 0x1c, 0, 0x04);
        self.registers.set_unk_discharge_mode(false);

        // todo: we really need to figure out what each power detector is for
        //
        // /* 0: PRE_DECT off */
        // rc = r82xx_write_reg_mask(priv, 0x06, 0, 0x40);
        self.registers.set_pwd_pdet3(false);

        // set AGC clock.
        // this is also set to 0b11 from the INIT array
        //
        // /* agc clk 250hz */
        // rc = r82xx_write_reg_mask(priv, 0x1a, 0x30, 0x30);
        self.registers.set_unk_agc_clk(0b11);

        // in librtlsdr there's a commented-out sleep here.
        // is this why they now set everything to different values?

        // set LNA TOP (again???)
        // /* write LNA TOP = 3 */
        // rc = r82xx_write_reg_mask(priv, 0x1d, 0x18, 0x38);
        // mask = 0b0011_1000
        self.registers.set_pdet1_gain(3);

        // set discharge mode
        //
        // mixer_top = 0x14;	/* mixer top:14 , top-1, low-discharge */
        // /*
        //  * write discharge mode
        //  * FIXME: IMHO, the mask here is wrong, but it matches
        //  * what's there at the original driver
        //  */
        // rc = r82xx_write_reg_mask(priv, 0x1c, mixer_top, 0x04);
        //
        // the mask looks fine
        self.registers.set_unk_discharge_mode(true);

        // set LNA discharge current??
        //
        // /* LNA discharge current */
        // lna_discharge = 14; // 0x0e = 0b0000_1110
        // rc = r82xx_write_reg_mask(priv, 0x1e, lna_discharge, 0x1f);
        // mask = 0b0001_1111
        self.registers.set_pdet_clk(14);

        // set AGC clock (again???)
        // /* agc clk 60hz */
        // rc = r82xx_write_reg_mask(priv, 0x1a, 0x20, 0x30);
        self.registers.set_unk_agc_clk(0b10);

        Ok(())
    }

    /// Select RF input
    ///
    /// Panics for R820T, if `input` is not [`Air`](RfInput::Air)
    pub fn select_rf_input(&mut self, input: RfInput) {
        assert!(
            matches!(input, RfInput::Air) || !matches!(self.r82xx.model, Model::R820T),
            "R820T only has one input RfInput::Air"
        );

        tracing::debug!(?input, "selecting RF input");

        let [cable_1, cable_2] = match input {
            RfInput::Air => [false, false],
            RfInput::Cable1 => [true, false],
            RfInput::Cable2 => [false, true],
        };

        self.registers.set_unk_cable_1_in(cable_1);
        self.registers.set_unk_cable_2_in(cable_2);
    }

    /// Determines selected RF input from register state.
    ///
    /// This returns an error if both [`Cable1`](RfInput::Cable1) and
    /// [`Cable2`](RfInput::Cable2) have been enabled, e.g. previously by
    /// explicitely writing to the registers.
    ///
    /// This won't read the registers from the device, as we initialize them to
    /// a known state, and they don't change on their own.
    pub fn selected_rf_input(&self) -> Result<RfInput, InvalidRfInputState> {
        let cable_1 = self.registers.unk_cable_1_in();
        let cable_2 = self.registers.unk_cable_2_in();

        match [cable_1, cable_2] {
            [false, false] => Ok(RfInput::Air),
            [true, false] => Ok(RfInput::Cable1),
            [false, true] => Ok(RfInput::Cable2),
            _ => Err(InvalidRfInputState { cable_1, cable_2 }),
        }
    }

    pub fn set_bandwidth(&mut self, bandwidth: f32) {
        tracing::debug!(?bandwidth, "setting bandwidth");

        self.set_filter_setting(filter_setting_for_bandwidth(bandwidth));
    }

    pub fn set_center_frequency(&mut self, center_frequency: f32) {
        tracing::debug!(?center_frequency, "setting center freqiency");

        todo!();
    }

    pub fn set_filter_setting(&mut self, filter_setting: &FilterSetting) {
        tracing::debug!(?filter_setting, "setting filter setting");

        self.registers.set_unk_filt_q(filter_setting.low_q);
        self.registers.set_unk_bw_1_7mhz(filter_setting.bw_1_7mhz);
        self.registers.set_filt_bw(filter_setting.filt_bw);
        self.if_frequency = filter_setting.center_frequency;
    }

    pub fn shutdown(&mut self) {
        tracing::debug!("setting {:?} to standby", self.model());

        self.shutdown_ours();
        //self.shutdown_librtlsdr();
    }

    #[allow(dead_code)]
    fn shutdown_librtlsdr(&mut self) {
        let writes = [
            (0x06, 0xb1),
            (0x05, 0xa0),
            (0x07, 0x3a),
            (0x08, 0x40),
            (0x09, 0xc0),
            (0x0a, 0x36),
            (0x0c, 0x35),
            (0x0f, 0x68),
            (0x11, 0x03),
            (0x17, 0xf4),
            (0x19, 0x0c),
        ];

        for (address, value) in writes {
            self.registers[address] = value;
        }
    }

    #[allow(dead_code)]
    fn shutdown_ours(&mut self) {
        // 0x05
        self.registers.set_pwd_lt(true); // turn off loop-through
        self.registers.set_pwd_lna1(true); // turn off lna 1
        self.registers.set_lna_gain(0); // set lna gain to min

        // 0x06
        self.registers.set_pwd_pdet1(true); // turn off pdet1
        self.registers.set_pwd_pdet3(false); // turn off pdet3
        self.registers.set_pw_lna(0b001); // don't know why librtlsdr sets this to 0b001, instead of 0b111(min)

        // 0x07
        self.registers.set_pwd_mix(false); // turn mixer off
        self.registers.set_pw0_mix(true); // mixer low power setting
        self.registers.set_mix_gain(0b1010); // don't know why librtlsdr sets this to 0b1010, instead of 0b0000(min)

        // 0x08
        self.registers.set_pwd_amp(false); // turn off amplifier
        self.registers.set_pw0_amp(true); // amplifier low power setting

        // 0x09
        self.registers.set_pwd_iffilt(true); // IF filter power off
        self.registers.set_pw1_iffilt(true); // IF filter low power setting

        // 0x0a
        self.registers.set_pwd_filt(false); // filter power off
        self.registers.set_pw_filt(0b01); // don't know why librtlsdr sets this to 0b01 instead of 0b11(min)
        self.registers.set_filt_code(0b0110); // librtlsdr sets this on standby

        // 0x0c
        self.registers.set_pwd_vga(false); // turn off vga
        self.registers.set_unk_pw0_vga(true); // low power setting
        self.registers.set_unk_adc_enable(false); // disable ADC
        self.registers.set_vga_code(0b0101); // librtlsdr sets this on standby

        // 0x11

        // todo: this is set to 0x68 for standby, but it's also initialized that way.
        // the spreadsheet lists ldo5vh and pwd_ldo_5v here that seem power-related, but
        // we never switch them(?). need to investigate these.
        //
        // also librtlsdr sets pwd_ldo_5v=0 for standby, but according to spreadsheet,
        // this means on.

        self.registers.set_pw_ldo_a(0);
        self.registers.set_unk_cp_cur(0);

        // 0x17
        self.registers.set_pw_ldo_d(0b11);
        self.registers.set_unk_div_buf_cur(0b11);
        self.registers.set_unk_pw_iq(0b10);

        // 0x19
        // librtlsdr sets ring_pw (0x19 [3:2]) to 0b11, but it's initialized as that.
        self.registers.set_pwd_rffilt(false); // turn off RF filter power
        self.registers.set_unk_rf_poly_filter_current(0);

        /*
        Note: Nvm, it's working now. We'll leave this here for a while, in case problems return.

        This still produces very strange behavior when trying to initialize the R828D after standby.
        Sometimes the R828D doesn't respond (USB timeout), but usually the DEMOD actually stalls (e.g. setting IIC_repeat fails).
        Sometimes the errors would appear on the second time sending to standby - meaning the whole init procedure worked after standby and then it broke.
        Sometimes even rtl_test will report errors after (demod, dropped sample). Though they go away after running it again.

        this is what we flush

        0000:   00 00 00 00  00 a0 b1 3a  40 c0 36 6b  35 53 75 68   .......:@.6k5Suh
        0010:   8c 03 06 31  84 72 1c f4  48 0c 68 00  24 dd 6e 40   ...1.r..H.h.$.n@

        this is what rtl_test dumps after standby (tuner_r82xx.c:1382)

        0000    __ __ __ __  __ a0 b1 3a  40 c0 36 af  35 53 75 68
        0010:   a4 03 06 31  05 38 ce f4  48 0c 68 00  24 dd 6e 40

                ours                   librtlsdr                initial
        0x0b:   0x6b, 0b0110_1011      0xaf, 0b1010_1111        0x6c, 0b0110_1100       filter settings
        0x10:   0x8c, 0b1000_1100      0xa4, 0b1010_0100        0x6c, 0b0110_1100       pll settings

        0x14..=0x16 are just tuning
         */
    }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        if self.warn_on_uncomitted_drop {
            if self.registers.modified != 0 {
                tracing::warn!(
                    modified = ?FormatRegisterBitMap(self.registers.modified),
                    "dropping Transaction with modified registers"
                );
            }
        }
    }
}

impl Tuner for R82xx {
    type Error = Error;

    fn name(&self) -> &str {
        self.model.name()
    }

    async fn set_bandwidth(&mut self, bandwidth: f32) -> Result<(), Self::Error> {
        let mut transaction = self.begin_transaction();
        transaction.set_bandwidth(bandwidth);
        transaction.commit().await
    }

    async fn set_center_frequency<'a>(
        &'a mut self,
        center_frequency: f32,
    ) -> Result<(), Self::Error> {
        let mut transaction = self.begin_transaction();
        transaction.set_center_frequency(center_frequency);
        transaction.commit().await
    }

    async fn shutdown(&mut self) -> Result<(), Self::Error> {
        let mut transaction = self.begin_transaction();
        transaction.shutdown();
        transaction.commit().await
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RfInput {
    /// RF-IN on R820T, Air-In on R828D
    Air,
    /// Cable1 on R828D
    Cable1,
    /// Cable2 on R828D
    Cable2,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Invalid RFInputState: cable_1={cable_1:?}, cable_2={cable_2:?}")]
pub struct InvalidRfInputState {
    pub cable_1: bool,
    pub cable_2: bool,
}

pub fn voltage_to_lna_vth(voltage: f32) -> u8 {
    ((voltage - VTH_MIN) / VTH_STEP).round().clamp(0.0, 15.0) as u8
}

pub const VTH_MIN: f32 = 0.34;
pub const VTH_MAX: f32 = 1.94;
pub const VTH_STEP: f32 = (VTH_MAX - VTH_MIN) / 15.0;

pub fn lna_vth_to_voltage(vth: u8) -> f32 {
    assert!(vth < 16);
    VTH_MIN + vth as f32 * VTH_STEP
}

/// Buffered register state of the R82xxx
///
/// This doesn't perform any actual reads or writes, but caches data locally. To
/// actually fetch registers from the tuner use [`Transaction::read`]. To write
/// all changed registers to the tuner use [`Transaction::flush`].
///
/// # Initialization
///
/// `librtlsdr` initializes ot as follows:
///
/// ```c
/// /* Those initial values start from REG_SHADOW_START (5) */
/// static const uint8_t r82xx_init_array[NUM_REGS] = {
/// 	0x83, 0x30, 0x75,			/* 05 to 07 */
/// 	0xc0, 0x40, 0xd6, 0x6c,			/* 08 to 0b */
/// 	0xf5, 0x63, 0x75, 0x68,			/* 0c to 0f */
/// 	0x6c, 0x83, 0x80, 0x00,			/* 10 to 13 */
/// 	0x0f, 0x00, 0xc0, 0x30,			/* 14 to 17 */
/// 	0x48, 0xcc, 0x60, 0x00,			/* 18 to 1b */
/// 	0x54, 0xae, 0x4a, 0xc0			/* 1c to 1f */
/// };
/// ```
///
/// We're doing this as well, but it's a bit hard to know what these do. We need
/// to replace these with actual calls to setters.
///
/// # Dump
///
/// Dump from testing of first 0x10 registers (from actual device):
///
/// ```plain
/// │00000000│ 69 01 01 ff ab 05 8d 5c ┊ 02 03 6c d6 ac ca ae 16 │i••××•×\┊••l××××•│
/// ```
#[derive(Clone, Copy, Default)]
pub struct RegisterBuffer {
    state: [u8; NUM_REGISTERS as usize],
    modified: u32,
}

impl RegisterBuffer {
    #[inline(always)]
    pub fn is_modified(&self, register: u8) -> bool {
        self.modified & (1 << register) != 0
    }
}

impl Index<u8> for RegisterBuffer {
    type Output = u8;

    #[inline(always)]
    fn index(&self, index: u8) -> &Self::Output {
        &self.state[usize::from(index)]
    }
}

impl Index<Range<u8>> for RegisterBuffer {
    type Output = [u8];

    #[inline(always)]
    fn index(&self, index: Range<u8>) -> &Self::Output {
        &self.state[usize::from(index.start)..usize::from(index.end)]
    }
}

impl IndexMut<u8> for RegisterBuffer {
    #[inline(always)]
    fn index_mut(&mut self, index: u8) -> &mut Self::Output {
        self.modified |= 1 << index;
        &mut self.state[usize::from(index)]
    }
}

impl IndexMut<Range<u8>> for RegisterBuffer {
    #[inline(always)]
    fn index_mut(&mut self, index: Range<u8>) -> &mut Self::Output {
        self.modified |= range_mask(index.start, index.end);
        &mut self.state[usize::from(index.start)..usize::from(index.end)]
    }
}

/// Register 0x15 and 0x16
impl RegisterBuffer {
    #[inline(always)]
    pub fn sdm_in(&self) -> u16 {
        u16::from_le_bytes(self[0x15..0x17].try_into().unwrap())
    }

    /// PLL fractional divider number input `SDM[16:1]`
    ///
    /// Register 0x15 = `SDM[8:1]`
    ///
    /// Register 0x16 = `SDM[16:9]`
    ///
    /// # TODO
    ///
    /// Decipher this:
    ///
    /// ```plain
    /// PLL fractional divider number input SDM[16:1]
    /// Nfra=SDM_IN[16]*2^-1+SDM_IN[15]*2^-2+E+SDM_IN[2]
    /// *2^-15+SDM_IN[1]*2^-16
    /// ```
    #[inline(always)]
    pub fn set_sdm_in(&mut self, value: u16) {
        self[0x15..0x17].copy_from_slice(&value.to_le_bytes());
    }
}

#[inline(always)]
fn range_mask(start: u8, end: u8) -> u32 {
    let end_mask = if end == 32 { u32::MAX } else { (1 << end) - 1 };
    let start_mask = (1 << start) - 1;

    end_mask ^ start_mask
}

/// Debug formatter for register bitmap
struct FormatRegisterBitMap(pub u32);

impl Debug for FormatRegisterBitMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();

        for i in 0..NUM_REGISTERS {
            if self.0 & (1 << i) != 0 {
                list.entry(&FormatRegisterAddress(i));
            }
        }

        list.finish()
    }
}

struct FormatRegisterAddress(pub u8);

impl Debug for FormatRegisterAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:02x}", self.0)
    }
}

macro_rules! registers {
    {$(
        $(#[$register_meta:meta])*
        $address:literal: {$(
            $(#[$field_meta:meta])*
            $name:ident: [$msb:literal $(: $lsb:literal)?],
        )*};
    )*} => {
        $(
            $(#[$register_meta])*
            #[doc = concat!("\n\nRegister ", stringify!($address))]
            impl RegisterBuffer {
                $(
                    registers!(@generate_impl($name, $address, [$msb, $($lsb)?], [$($field_meta),*]));
                )*
            }
        )*


        impl Debug for RegisterBuffer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut s = f.debug_struct("RegisterBuffer");
                s.field("_state", &self.state);
                $($(
                    s.field(stringify!($name), &self.$name());
                )*)*
                s.finish()
            }
        }

        #[cfg(test)]
        #[test]
        fn no_fields_overlap() {
            let mut registers = std::collections::HashMap::<(u8, u8), &'static str>::default();
            $($({
                registers!(@generate_test(registers, $name, $address, [$msb, $($lsb)?], [$($field_meta),*]));
            })*)*
        }
    };
    (@generate_impl($name:ident, $address:literal, [$bit:literal,], [$($meta:meta),*])) => {
        $(#[$meta])*
        #[doc = concat!("\n\nRegister ", stringify!($address), ", Bit ", $bit)]
        #[inline(always)]
        pub fn $name(&self)  -> bool {
            ::bitfield::Bit::bit(&self[$address], $bit)
        }

        paste! {
            $(#[$meta])*
            #[doc = concat!("\n\nRegister ", stringify!($address), ", Bit ", $bit)]
            #[inline(always)]
            pub fn [<set_ $name>](&mut self, value: bool) {
                ::bitfield::BitMut::set_bit(&mut self[$address], $bit, value);
            }
        }
    };
    (@generate_impl($name:ident, $address:literal, [$msb:literal,$lsb:literal], [$($meta:meta),*])) => {
        $(#[$meta])*
        #[doc = concat!("\n\nRegister ", stringify!($address), ", Bits ", $msb, ":", $lsb)]
        #[inline(always)]
        pub fn $name(&self)  -> u8 {
            ::bitfield::BitRange::<u8>::bit_range(&self[$address], $msb, $lsb)
        }

        paste! {
            $(#[$meta])*
            #[doc = concat!("\n\nRegister ", stringify!($address), ", Bits ", $msb, ":", $lsb)]
            #[inline(always)]
            pub fn [<set_ $name>](&mut self, value: u8) {
                ::bitfield::BitRangeMut::<u8>::set_bit_range(&mut self[$address], $msb, $lsb, value);
            }
        }
    };
    (@generate_test($registers:expr, $name:ident, $address:literal, [$bit:literal,], [$($meta:meta),*])) => {
        if let Some(used_by) = $registers.get(&($address, $bit)) {
            panic!("Field {} also used by {used_by} at bit {}", stringify!($name), $bit);
        }
        $registers.insert(($address, $bit), stringify!($name));
    };
    (@generate_test($registers:expr, $name:ident, $address:literal, [$msb:literal,$lsb:literal], [$($meta:meta),*])) => {
        assert!($msb > $lsb, "Invalid bitrange for {} 0x{:02x} [{}:{}]", stringify!($name), $address, $msb, $lsb);

        for bit in $lsb..=$msb {
            if let Some(used_by) = $registers.get(&($address, bit)) {
                panic!("Field {} also used by {used_by} at bit {bit} in register 0x{:02x}", stringify!($name), $address);
            }
            $registers.insert(($address, bit), stringify!($name));
        }
    };
}

registers! {
    0x00: {
        /// For probing - always 0x69
        ///
        /// Read-only
        test: [7:0],
    };
    0x01: {
        /// ADC on output from PDET3
        ///
        /// and possibly other options?
        ///
        /// Read-only
        unk_adc_output: [5:0],
    };
    0x02: {
        unk_autotune_done: [7],
        /// VCO indicator
        ///
        /// Read-only
        ///
        /// TODO: split?
        ///
        /// ```plain
        /// vco_lock: [6],
        /// vco_core: [5],
        /// vco_band: [4:0],
        /// ```
        vco_indicator: [6:0],
    };
    0x03: {
        /// RF indicator
        ///
        /// Read-only
        ///
        /// TODO: Split?
        ///
        /// ```plain
        /// mixer_gain: [7:4],
        /// lna_gain: [0:3],
        /// ```
        rf_indicator: [7:0],
    };
    0x04: {
        /// VCO fine tune
        ///
        /// Not in datasheet, but in librtlsdr.
        ///
        /// `vco_fine_tune = (data[4] & 0x30) >> 4;`
        ///
        /// `vco_power_ref` is set to 1 for any R828D, or any Blog V4L.
        ///
        /// Controls `div_num` (in 0x10):
        ///
        /// ```c
        /// if (vco_fine_tune > vco_power_ref)
        ///     div_num = div_num - 1;
        /// else if (vco_fine_tune < vco_power_ref)
        ///     div_num = div_num + 1;
        /// ```
        unk_vco_fine_tune: [5:4],
        /// Filter calibration code
        ///
        /// Not in datasheet, but in librtlsdr.
        ///
        /// See `r82xx_set_tv_standard`.
        unk_fil_cal_code: [3:0],
    };
    0x05: {
        /// Loop through - 0=on, 1=off
        pwd_lt: [7],
        /// Not in datasheet, but used by librtlsdr
        ///
        /// Referred to as `cable_1_in`. It's 1=on if VHF band is used. In the datasheet this is fixed to 0.
        ///
        /// ```c
        /// /* activate cable 1 (VHF input) */
        /// cable_1_in = (band == VHF) ? 0x40 : 0x00;
        /// rc = r82xx_write_reg_mask(priv, 0x05, cable_1_in, 0x40);
        /// ```
        ///
        /// This might be the second LNA that is mentioned for the R828D.
        unk_cable_1_in: [6],
        /// LNA 1 power control - 0=on, 1=off
        ///
        /// Also referred to as `air_in` in librtlsdr. It's 0=on if UHF band is used.
        /// We think this doesn't turn on `air_in`, but rather just disables the LNA, if `cable_1_in` or `cable_2_in` are used.
        ///
        /// ```c
        /// /* activate air_in (UHF input) */
        /// air_in = (band == UHF) ? 0x00 : 0x20;
        /// rc = r82xx_write_reg_mask(priv, 0x05, air_in, 0x20);
        /// ```
        pwd_lna1: [5],
        /// LNA gain mode switch - 0=auto, 1=manual
        lna_gain_mode: [4],
        /// LNA manual gain control - 15(max) .. 0(min)
        lna_gain: [3:0],
    };
    0x06: {
        /// Power detector 1 - 0=on, 1=off
        pwd_pdet1: [7],
        /// Power detector 3 - 0=on, 1=off
        ///
        /// The spreadsheet calls this `PWD_PDET2`. I think the datasheet is right, but ya never know.
        pwd_pdet3: [6],
        /// Filter gain 3dB - 0=0dB, 1=+3dB
        filt_3db: [5],
        /// Not in datasheet, but used by librtlsdr.
        ///
        /// ```c
        /// filt_gain = 0x30;	/* +3db, 6mhz on, 0b0011_0000 */
        /// /* Set filt_3dB, V6MHz */
        /// rc = r82xx_write_reg_mask(priv, 0x06, filt_gain, 0x30);
        /// ```
        unk_v6mhz: [4],
        /// Not in datasheet, but used by librtlsdr.
        ///
        /// Referred to as `cable_2_in`. It's 1=on if HF band is used. In the datasheet this is fixed to 0.
        ///
        /// ```c
        /// /* activate cable 2 (HF input) */
        /// cable_2_in = (band == HF) ? 0x08 : 0x00;
        /// rc = r82xx_write_reg_mask(priv, 0x06, cable_2_in, 0x08);
        /// ```
        unk_cable_2_in: [3],
        /// LNA power control - 000(max) .. 111(min)
        pw_lna: [2:0],
    };
    0x07: {
        /// Undocumented
        ///
        /// ```c
        /// img_r = 0x00;		/* image negative */
        /// /* Set Img_R */
        /// rc = r82xx_write_reg_mask(priv, 0x07, img_r, 0x80);
        /// ```
        unk_img_r: [7],
        /// Mixer power - 0=on, 1=off
        pwd_mix: [6],
        /// Mixer current control - 0=max current, 1=normal current
        pw0_mix: [5],
        /// Mixer gain mode - 0=manual, 1=auto
        mixgain_mode: [4],
        /// Mixer manual gain control - 0000(min) .. 1111(max)
        mix_gain: [3:0],
    };
    0x08: {
        /// Mixer buffer power - 0=off, 1=on
        pwd_amp: [7],
        /// Mixer buffer current setting - 0=high, 1=low
        pw0_amp: [6],
        /// Image gain adjustment - 0(min) .. 63(max)
        imr_g: [5:0],
    };
    0x09: {
        /// IF Filter power - 0=on, 1=off
        pwd_iffilt: [7],
        /// IF Filter current - 0=high, 1=low
        pw1_iffilt: [6],
        /// Image phase adjustment - 0(min) .. 63(max)
        imr_p: [5:0],
    };
    0x0a: {
        /// Filter power - 0=off, 1=on
        pwd_filt: [7],
        /// Filter power control - 00=high .. 11=low
        pw_filt: [6:5],
        /// Undocumented, but in librtlsdr.
        ///
        /// This is always set to 1 in `r82xx_set_tv_standard`:
        ///
        /// ```c
        /// filt_q = 0x10;		/* r10[4]:low q(1'b1) */
        /// ```
        ///
        /// Set also by `r82xx_set_bandwidth`
        ///
        /// Spreadsheet: "Low-Q: should give less sharp filter edges"
        unk_filt_q: [4],
        /// Filter bandwidth manual fine tune - 0000=widest, 1111=narrowest
        ///
        /// This can be calibrated. See field `unk_fil_cal_code` and librtlsdr function `r82xx_set_tv_standard`.
        filt_code: [3:0],
    };
    0x0b: {
        /// Undocumented
        ///
        /// Seems to enable filtering with < 1.7 MHz bandwidth
        unk_bw_1_7mhz: [7],
        /// Filter bandwidth manual coarse tunnel - 00=widest, 10 or 01=middle, 11=narrowest
        filt_bw: [6:5],
        /// Undocumented
        ///
        /// Is this the bit that triggers calibration?
        unk_cal_trig: [4],
        /// High pass filter corner control - 0000=high, 1111=low
        hpf: [3:0],
    };
    0x0c: {
        /// Undocumented
        ///
        /// In spreadsheet: 0=on, 1=off
        unk_adc_enable: [7],
        /// VGA power control - 0=off, 1=on
        pwd_vga: [6],
        /// VGA power setting (not in datasheet)
        ///
        /// If it's like the other power settings 0=high, 1=low. This is confirmed because librtlsdr sets this to 1 when going into standby.
        unk_pw0_vga: [5],
        /// VGA gain manual / pin selector - 1=IF vga gain controlled by vagc pin, 0=IF vga gain controlled by `vga_code[5:0]`
        vga_mode: [4],
        /// IF VGA manual gain control - 0000=-12 dB, 1111=40.5 dB; -3.5 dB/step
        vga_code: [3:0],
    };
    0x0d: {
        /// LNA AGC power detector voltage threshold high setting - 1111=1.94V, 0000=0.34V; ~0.1V/step
        lnavth_h: [7:4],
        /// LNA AGC power detector voltage threshold low setting - 1111=1.94V, 0000=0.34V; ~0.1V/step
        lnavth_l: [3:0],
    };
    0x0e: {
        /// Mixer AGC power detector voltage threshold high setting - 1111=1.94V, 0000=0.34V; ~0.1V/step
        mixvth_h: [7:4],
        /// Mixer AGC power detector voltage threshold low setting - 1111=1.94V, 0000=0.34V; ~0.1V/step
        mixvth_l: [3:0],
    };
    0x0f: {
        /// Not in datasheet.
        ///
        /// In the datasheet this is always 0
        ///
        /// librtlsdr:
        ///
        /// ```c
        /// flt_ext_widest = 0x00;	/* r15[7]: flt_ext_wide off */
        /// rc = r82xx_write_reg_mask(priv, 0x0f, flt_ext_widest, 0x80);
        /// ```
        ///
        /// so, 1=on, 0=off
        unk_filt_ext_widest: [7],
        /// Clock out pin control - 0=on, 1=off (clk output)
        clk_out_enb: [4],
        /// AGC clk control - 0=on, 1=off (internal agc clock)
        clk_agc_enb: [1],
    };
    0x10: {
        /// PLL to Mixer divider number control
        ///
        /// - 000: mixer in = vco out / 2
        /// - 001: mixer in = vco out / 4
        /// - 010: mixer in = vco out / 8
        /// - 011: mixer in = vco out
        sel_div: [7:5],
        /// PLL Reference frequency divider
        ///
        /// - 0: fref=xtal_freq
        /// - 1: fref=xtal_freq / 2 (for Xtal > 24 MHz)
        ///
        ref_div2: [4],
        /// Unknown related to capx
        ///
        /// This is set by librtlsdr together with capx via `r82xx_xtal_cap_value` enum.
        ///
        /// ```plain
        /// XTAL_LOW_CAP_30P: 0x0b: 0b0_1011
        /// XTAL_LOW_CAP_20P: 0x02: 0b0_0010
        /// XTAL_LOW_CAP_10P: 0x01: 0b0_0001
        /// XTAL_LOW_CAP_0P:  0x00: 0b0_0000
        /// XTAL_HIGH_CAP_0P: 0x10: 0b1_0000
        /// ```
        ///
        /// But this mis masked with `0b1011` and ored with `0b1000` if it's anything but `XTAL_HIGH_CAP_0P`.
        ///
        /// They never set anything other than `XTAL_HIGH_CAP_0P` anyway. Although there is an unused function to test for the best values.
        unk_drive: [3],
        /// Unknown - always 1
        unk_cap: [2],
        /// Internal xtal cap setting - 00=no cap, 01=10pF, 10=20pF, 11=30pF
        capx: [1:0],
    };
    0x11: {
        /// PLL analog low drop out regulator switch - 00=off, 01=2.1V, 10=2.0V, 11=1.9V
        pw_ldo_a: [7:6],
        /// Not in datasheet
        ///
        /// In librtlsdr:
        ///
        /// ```c
        /// cp_cur = 0x38;		/* 111, auto */
        /// rc = r82xx_write_reg_mask(priv, 0x11, cp_cur, 0x38);
        /// mask = 0b0011_1000
        /// ```
        ///
        /// CP pin - PLL charge pump?
        ///
        /// Set to 0b000 for standby. So the 0b000=low/off, 0b111=highest maybe?
        ///
        /// The spreadsheet lists this as 0=on
        unk_cp_cur: [5:3],

        /// Undocumented
        ///
        /// No idea what this does.
        ///
        /// It's initialized as 0b11 and set to 0b11 on standby. Spreadsheet just names it.
        unk_bias_hf: [1:0],
    };
    0x12: {
        /// Not in datasheet
        ///
        /// 0b000 is max?
        ///
        /// ```c
        /// /* set VCO current = 100 */
        /// /* rc = r82xx_write_reg_mask(priv, 0x12, 0x80, 0xe0); */
        /// /* RTL-SDR Blog Modification: Set VCO current to MAX */
        /// rc = r82xx_write_reg_mask(priv, 0x12, 0x06, 0xff);
        /// ```
        unk_vco_current: [7:5],
        /// Not in datasheet
        unk_dis_dither: [4],
        /// In datasheet, but no description
        pw_sdm: [3],
        /// Not in datasheet
        ///
        /// CP pin - PLL charge pump?
        unk_cp_offset: [2:1],
        /// Not in datasheet
        ///
        /// CP pin - PLL charge pump?
        unk_cp_0406: [0],
    };
    0x13: {
        /// PLL auto_tune clk
        ///
        /// 0=on, 1=off
        ///
        /// Undocumented, but used by librtlsdr
        ///
        /// See `r82xx_xtal_check`
        unk_auto_tune: [7],
        /// Undocumented
        ///
        /// 1=on, 0=off
        unk_band_force: [6],
        /// Undocumented, but used by librtlsdr.
        ///
        /// This is usually set to 0x31. In `r82xx_xtal_check` it is set to 0x3f.
        unk_vco_band: [5:0],
    };
    0x14: {
        /// PLL integer divider number input Si2c
        ///
        /// Nint = 4 * Ni2c + Si2c + 13
        /// PLL divider number Ndiv = (Nint + Nfra) * 2
        s_i2c:[7:6],
        /// PLL integer divider number input Ni2c
        n_i2c:[5:0],
    };
    0x17: {
        /// PLL digital low drop out regulator supply current switch:
        ///
        /// - 00: 1.8V, 8mA
        /// - 01: 1.8V, 4mA
        /// - 10: 2.0V, 8mA
        /// - 11: off
        pw_ldo_d: [7:6],
        /// Not in datasheet
        ///
        /// in librtlsdr:
        ///
        /// ```c
        /// div_buf_cur = 0x20;	/* 10, 200u */
        /// div_buf_cur = 0x30;	/* 11, 150u */
        /// /* RTL-SDR Blog Hack. Improve L-band performance by setting PLL drop out to 2.0v */
        /// div_buf_cur = 0xa0;  // 0b10, because it's masked
        /// rc = r82xx_write_reg_mask(priv, 0x17, div_buf_cur, 0x30);
        /// ```
        ///
        /// Set to 0b11 for standby.
        ///
        /// Looks like:
        ///  - 0b11 = 150 uA
        ///  - 0b10 = 200 uA
        ///  - 0b01 = 250 uA (extrapolated)
        ///  - 0b00 = 300 uA (extrapolated)
        unk_div_buf_cur: [5:4],
        /// Open drain - 0=high-z, 1=low-z
        open_d: [3],
        /// Not in datasheet
        ///
        /// librtlsdr: initializes to 0b00, sets to 0b10 for standby.
        ///
        /// spreadsheet labels this "1 0 pw_IQ" in power group.
        unk_pw_iq: [2:1],
        /// Not in datasheet
        ///
        /// librtlsdr: initializes to 0, sets to 0 for standby.
        ///
        /// spreadsheet labels this "1 0 pw_IQ" in power group. Also labels it as 0=on
        unk_pwd_iq: [0],
    };
    0x19: {
        /// RF filter power - 0=off, 1=on
        pwd_rffilt: [7],
        /// RF poly filter current
        ///
        /// Not in datasheet - there it's always `0b00`.
        ///
        /// In librtlsdr `r82xx_set_tv_standard`:
        ///
        /// ```c
        /// polyfil_cur = 0x60;	/* r25[6:5]:min */
        /// /* RF poly filter current */
        /// rc = r82xx_write_reg_mask(priv, 0x19, polyfil_cur, 0x60);
        /// ```
        ///
        /// So is `0b11` minimum then?
        ///
        /// `r82xx_standby` sets this to `0b00`:
        ///
        /// ```c
        /// rc = r82xx_write_reg(priv, 0x19, 0x0c);
        /// ```
        ///
        /// Spreadsheet splits this into "ring_cp_current" and "POLYFIL_CUR"
        unk_rf_poly_filter_current: [6:5],
        /// Switch agc_pin
        ///
        /// - 0: agc = agc_in
        /// - 1: agc = agc_in2
        sw_agc: [4],
    };
    0x1a: {
        /// Tracking filter switch - 00=TF on, 01=bypass
        rfmux: [7:6],
        /// AGC clock (not in datasheet)
        ///
        /// In librtlsdr:
        ///
        /// ```c
        /// /* agc clk 250hz */
        /// rc = r82xx_write_reg_mask(priv, 0x1a, 0x30, 0x30);
        /// /* agc clk 60hz */
        /// rc = r82xx_write_reg_mask(priv, 0x1a, 0x20, 0x30);
        /// /* agc clk 1Khz, external det1 cap 1u */
        /// rc = r82xx_write_reg_mask(priv, 0x1a, 0x00, 0x30);
        /// ```
        ///
        /// In datasheet this is fixed as 0b10
        ///
        /// - 0b00 - 1 kHz
        /// - 0b10 - 60 Hz
        /// - 0b11 = 250 Hz
        unk_agc_clk: [5:4],
        /// PLL auto tune clock rate - 00=128kHz, 01=32kHz, 10=8kHz
        pll_auto_clk: [3:2],
        /// RF filter band selection, 00=highest, 01=medium, 10=low
        rffilt: [1:0],
    };
    0x1b: {
        /// 0000 highest corner for LPNF; 1111 lowest corner for LPNF
        tf_nch: [7:4],
        /// 0000 highest corner for LPF; 1111 lowest corner for LPF
        tf_lp: [3:0],
    };
    0x1c: {
        /// Power detector 3 TOP(take off point) control - 0=highest .. 15=lowest
        pdet3_gain: [7:4],
        unk_lna_top_p1: [3],
        /// Not in datasheet
        ///
        /// in librtlsdr:
        ///
        /// ```c
        /// // /* 0: normal mode */
        //  rc = r82xx_write_reg_mask(priv, 0x1c, 0, 0x04);
        /// ```
        unk_discharge_mode: [2],
        /// 0=rf, 1=ring
        unk_mixer_src: [1],
        unk_vco_out: [0],
    };
    0x1d: {
        /// Undocumented, but set by librtlsdr
        ///
        /// ```c
        /// lna_top = 0xe5;		/* detect bw 3, lna top:4, predet top:2 */
        /// rc = r82xx_write_reg_mask(priv, 0x1d, lna_top, 0xc7);
        /// mask    = 0b1100_0111
        /// lna_top = 0b1110_0101
        /// ```
        ///
        /// Reasonable that they split it this way `0b11, 100, 101`, which corresponds to datasheet.
        ///
        /// So "detect bw" is this undocumented field.
        /// "lna top" is `pdet1_gain`, but it's masked out when they set it.
        /// And "predet top" is `pdet2_gain`, but they actually set it to 5.
        ///
        unk_detect_bw: [7:6],
        /// Power detector 1 TOP(take off point) control - 0=highest .. 15=lowest
        pdet1_gain: [5:3],
        /// Power detector 2 TOP(take off point) control - 0=highest .. 15=lowest
        pdet2_gain: [2:0],
    };
    0x1e: {
        /// Filter extension under weak signal - 0=off, 1=on
        ///
        /// Not mentioned in the datasheet's register matrix, but in the descriptions. In the matrix it's always 1.
        filter_ext: [6],
        /// Undocumented
        ///
        /// This is also part of [`pdet_clk`](Self::pdet_clk), but librtlsdr uses it separately and the spreadsheet lists is separately.
        ///
        /// In `r82xx_set_tv_standard`:
        ///
        /// ```c
        /// ext_enable = 0x60;	/* r30[6]=1 ext enable; r30[5]:1 ext at lna max-1 */
        /// ```
        ext_enable: [5],
        /// Power detector timing control - 111111=max, 000000=min
        pdet_clk: [4:0],
    };
    0x1f: {
        /// Not in datasheet
        ///
        /// In librtlsdr:
        ///
        /// ```c
        /// lt_att = 0x00;		/* r31[7], lt att enable */
        /// rc = r82xx_write_reg_mask(priv, 0x1f, lt_att, 0x80);
        /// ```
        ///
        /// So, 0=on, 1=off?
        ///
        /// In the datasheet this is always 1
        unk_lt_att: [7],
    };

}
