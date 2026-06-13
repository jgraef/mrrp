//! Rafael R820T and R828D tuners
//!
//! ![R820T block diagram](http://superkuh.com/LB6MI-r820t-blocks.jpg)
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

pub mod blog;
pub mod preset;
mod register;
mod types;

use std::{
    fmt::Debug,
    ops::Range,
};

use pretty_hex::PrettyHex;

pub use crate::tuner::r82xx::{
    register::*,
    types::*,
};
use crate::{
    rtl2832u::{
        self,
        Rtl2832u,
        i2c::I2cDevice,
    },
    tuner::{
        IfSetting,
        Tuner,
        TunerError,
        TunerProbe,
        r82xx::preset::{
            bandwidth_setting,
            frequency_setting,
        },
    },
};

// todo: remove this. the IF frequency depends on the bandwidth
pub const DEFAULT_IF_FREQUENCY: u32 = 3570000;

pub const DEFAULT_CRYSTAL_FREQ: u32 = 16000000;

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

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Rtl2823u(#[from] rtl2832u::Error),

    #[error(transparent)]
    I2cDeviceBusy(#[from] rtl2832u::i2c::I2cDeviceBusy),

    #[error(transparent)]
    NoPllConfig(#[from] NoPllConfig),
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("No suitable PLL configuration found for LO frequency: {lo_frequency}")]
pub struct NoPllConfig {
    pub lo_frequency: f32,
    pub sel_div: SelDiv,
    pub vco_frequency: f32,
    pub crystal_frequency: f32,
    pub vco_power_ref: u8,
}

impl TunerError for Error {}

#[derive(Clone, Debug)]
pub struct R82xxProbe;

impl R82xxProbe {
    /// The models we probe for.
    ///
    /// We can't reliably probe for all of them, since they share I2C addresses.
    /// We should prefer device enumeration to select the appropriate probe.
    pub const DEFAULT_MODELS: &[Model] = &[Model::R820T, Model::R828D];
}

impl TunerProbe for R82xxProbe {
    type Error = Error;
    type Tuner = R82xx;

    async fn try_open(&self, rtl2832u: &mut Rtl2832u) -> Result<Option<Self::Tuner>, Self::Error> {
        for model in Self::DEFAULT_MODELS {
            tracing::debug!("probing for {}", model.name());

            let mut i2c_device = rtl2832u.try_open_i2c(model.i2c_address())?.with_repeater();

            // According to datasheet this is 0x96, but the chip sends data from LSB
            // to MSB, while the RTL2832U decodes it the other way.

            if let Ok(data) = i2c_device.read(rtl2832u, 1).await
                && data[0] == 0x69
            {
                tracing::debug!("{} found", model.name());

                let mut r82xx = R82xx::new(i2c_device, *model);

                let mut transaction = r82xx.begin_transaction(rtl2832u);
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
    crystal_frequency: f32,
    crystal_config: CrystalConfig,
}

impl R82xx {
    pub fn new(i2c_device: I2cDevice, model: Model) -> Self {
        // librtlsdr has a commented-out `r82xx_xtal_check` in `r82xx_init`. I think it
        // selects the appropriate capacitor and drive for the crystal.

        let crystal_config = CrystalConfig {
            capacitor: CrystalCapacitor::P0,
            drive: CrystalDrive::High,
        };

        Self {
            model,
            i2c_device,
            register_state: Default::default(),
            if_frequency: DEFAULT_IF_FREQUENCY as f32,
            crystal_frequency: DEFAULT_CRYSTAL_FREQ as f32,
            crystal_config,
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
    pub fn begin_transaction<'a>(&'a mut self, rtl2832u: &'a mut Rtl2832u) -> Transaction<'a> {
        let registers = RegisterBuffer::from_state(self.register_state);
        let if_frequency = self.if_frequency;

        Transaction {
            r82xx: self,
            rtl2832u,
            registers,
            if_frequency,
            warn_on_uncomitted_drop: true,
        }
    }
}

#[derive(Debug)]
pub struct Transaction<'a> {
    r82xx: &'a mut R82xx,
    rtl2832u: &'a mut Rtl2832u,
    registers: RegisterBuffer,
    if_frequency: f32,
    warn_on_uncomitted_drop: bool,
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
        let data = self.r82xx.i2c_device.read(self.rtl2832u, n.into()).await?;

        for i in 0..n {
            if !self.registers.is_modified(i) {
                // R82xx sends bytes with bits reversed.
                let value = data[usize::from(i)].reverse_bits();

                // Don't deref_mut into registers directly to avoid setting the dirty bit.
                self.registers.set_no_dirty(i, value);

                // also write into backing storage
                self.r82xx.register_state[usize::from(i)] = value;
            }
        }

        tracing::debug!(registers = ?self.registers[0..n], "read registers");

        Ok(())
    }

    pub async fn commit(mut self) -> Result<(), Error> {
        self.flush().await
    }

    pub fn discard(mut self) {
        self.warn_on_uncomitted_drop = false;
    }

    /// Write out any dirty registers
    ///
    /// This will ever only write registers that are marked as dirty, but it'll
    /// try to do so in as few write commands as possible.
    pub async fn flush(&mut self) -> Result<(), Error> {
        tracing::debug!(
            "flushing registers\nmodified: {:?}\n{:?}",
            self.registers.modified(),
            self.registers.state().hex_dump()
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

            self.r82xx.i2c_device.write(self.rtl2832u, &command).await?;

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
        self.r82xx.register_state = *self.registers.state();

        // clear all modified flags
        self.registers.clear_modified();

        // commit IF frequency
        self.r82xx.if_frequency = self.if_frequency;

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
        self.registers.set_pwd_vga(true);
        // this is in librtlsdr's init function
        // on, controlled by vagc pin
        //self.registers.set_vga_mode(true);
        //self.registers.set_vga_code(0);
        // or shorter: self.set_vga(Vga::Pin);

        // but they *always* set the vga to a fixed 16 dB in r82xx_freq
        self.set_vga_gain(VgaGain::from_code(0x08).unwrap());

        // when they set the vga gain in r82xx_freq they also always enable the ADC
        //
        // todo: this this ADC even needed when we set the VGA gain via code? our
        // understanding is that this ADC reads the VAGC pin. if that is the case, move
        // this into `set_vga_gain`.
        self.registers.set_unk_adc_enable(false);

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
        // cable1_in=false, cable2_in=false, pwd_lna1=false
        // pwd_lna1=false means on, it's also known as air_in
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

        if let Ok(selected) = self.selected_rf_input()
            && selected == input
        {
            // early exit if we wouldn't change anything. this would not cause a register
            // write anyway, but we don't want to spam the logs
            return;
        }

        tracing::debug!(?input, "selecting RF input");

        let [cable_1, cable_2, pwd_lna1] = match input {
            RfInput::Air => [false, false, false],
            RfInput::Cable1 => [true, false, true],
            RfInput::Cable2 => [false, true, true],
        };

        self.registers.set_unk_cable_1_in(cable_1);
        self.registers.set_unk_cable_2_in(cable_2);
        self.registers.set_pwd_lna1(pwd_lna1);
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

        self.set_if_filter_setting(&bandwidth_setting(bandwidth).if_filter);
    }

    pub fn set_if_filter_setting(&mut self, filter_setting: &IfFilterSetting) {
        tracing::debug!(?filter_setting, "setting IF filter setting");

        self.registers.set_unk_filt_q(filter_setting.low_q);
        self.registers.set_unk_bw_1_7mhz(filter_setting.bw_1_7mhz);
        self.registers.set_filt_bw(filter_setting.filt_bw);
        self.registers.set_hpf(filter_setting.hpf);
        self.if_frequency = filter_setting.if_frequency;
    }

    pub fn set_center_frequency(&mut self, center_frequency: f32) -> Result<(), Error> {
        let lo_frequency = center_frequency + self.if_frequency;

        // todo: this configures the tracking filter, so why would this use the LO
        // frequency?
        //
        // we think the crystal config might depend on the LO frequency, but the
        // tracking filter should depend on the RF frequency.
        let setting = frequency_setting(lo_frequency);

        tracing::debug!(
            ?center_frequency,
            if_frequency = ?self.if_frequency,
            ?lo_frequency,
            ?setting,
            "setting center frequency"
        );

        self.set_crystal_config(setting.crystal_capacitor);

        // configure tracking filter
        self.set_tracking_filter_setting(&setting.tracking_filter);

        // r82xx_set_freq sets the vga gain here to a fixed "16.3 dB"
        //
        // r82xx_set_vga_gain always sets it to 16.3 dB
        // rc = r82xx_write_reg_mask(priv, 0x0c, 0x08, 0x9f); // 16.3 dB
        //
        // if this always set like this, we can also just do this in initialize.
        // but we should test this.
        //
        // also code 0x8 is 16 dB, not 16.3 dB
        //
        // this also turns on the ADC (unk_adc_enable=false). the reg init array has
        // this true, so we set this to false in initialize
        //
        //self.registers.set_unk_adc_enable(false); // on,

        self.set_pll(lo_frequency)?;

        Ok(())
    }

    pub fn set_vga_gain(&mut self, gain: VgaGain) {
        match gain {
            VgaGain::Pin => {
                self.registers.set_vga_mode(true);
            }
            VgaGain::Code(code) => {
                self.registers.set_vga_mode(false);
                self.registers.set_vga_code(code.into());
            }
        }
    }

    pub fn set_lna_gain(&mut self, gain: LnaGain) {
        match gain {
            LnaGain::Auto => {
                self.registers.set_lna_gain_mode(false);
            }
            LnaGain::Code(code) => {
                self.registers.set_lna_gain_mode(true);
                self.registers.set_lna_gain(code.into());
            }
        }
    }

    pub fn set_mix_gain(&mut self, gain: MixGain) {
        match gain {
            MixGain::Auto => {
                self.registers.set_mixgain_mode(true);
            }
            MixGain::Code(code) => {
                self.registers.set_mixgain_mode(false);
                self.registers.set_mix_gain(code.into());
            }
        }
    }

    pub fn set_crystal_config(&mut self, capacitor: CrystalCapacitor) {
        // set crystal capacitor
        //
        // we don't think librtlsdr ever sets anything but 0pF/high here
        //
        // also pretty sure that switch in `r82xx_set_mux` just selects the minimum of
        // both, considering the values they have in `freq_ranges`.
        let effective_cystal_config = CrystalConfig {
            capacitor: capacitor.min(self.r82xx.crystal_config.capacitor),
            drive: self.r82xx.crystal_config.drive,
        };

        tracing::debug!(crystal_config = ?self.r82xx.crystal_config, ?effective_cystal_config);

        self.registers
            .set_capx(effective_cystal_config.capacitor.into());
        self.registers
            .set_unk_drive(effective_cystal_config.drive.into());
    }

    pub fn set_tracking_filter_setting(&mut self, setting: &TrackingFilterSetting) {
        tracing::debug!(?setting, "configuring tracking filter");

        self.registers.set_open_d(setting.open_d.into());
        self.registers.set_rfmux(setting.rf_mux.into());
        self.registers.set_rffilt(setting.rf_filt.into());
        self.registers.set_tf_nch(setting.tf_nch);
        self.registers.set_tf_lp(setting.tf_lp);
    }

    pub fn set_pll(&mut self, lo_frequency: f32) -> Result<(), NoPllConfig> {
        self.registers.set_ref_div2(false);

        // 0x12 at init = 0x80
        //
        // /* set VCO current = 100 */
        // /* rc = r82xx_write_reg_mask(priv, 0x12, 0x80, 0xe0); */
        //
        // /* RTL-SDR Blog Modification: Set VCO current to MAX */
        // rc = r82xx_write_reg_mask(priv, 0x12, 0x06, 0xff);
        //
        // so the original code didn't touch the other bits and set vco_current to 0b100
        //
        // vco_current = 0b000 (set to 0b000 in initialize)
        // dis_dither = false (false in reg init array)
        // pw_sdm = false (true in reg init array), on=false
        // cp_offset = 0b11 (set to 0b11 in initialize)
        // cp_0406 = false (false in reg init)

        // Set VCO current to max
        self.registers.set_unk_vco_current(0b000);

        // SDM power on
        self.registers.set_pw_sdm(false);

        // check that we get the same register value as librtlsdr
        //
        // todo: remove
        assert_eq!(self.registers[0x12], 0x06);

        let sel_div = SelDiv::from_frequency(lo_frequency);

        // todo: librtlsdr adjusts the sel_div value using vco_fine_tune, though we
        // haven't actually observed this taking effect (we tuned a bit while logging
        // some values in r82xx_set_pll).

        self.registers.set_sel_div(sel_div.into());

        // the VCO frequency we want
        let vco_frequency = lo_frequency * sel_div.effective_divider();

        tracing::debug!(?sel_div, ?vco_frequency, crystal_frequency = ?self.r82xx.crystal_frequency);

        // caculate PLL divider settings
        let pll_divider = PllDivider::from_vco_frequency(
            vco_frequency,
            self.r82xx.crystal_frequency,
            self.r82xx.model.vco_power_ref(),
        )
        .ok_or_else(|| {
            tracing::warn!(
                ?vco_frequency,
                crystal_frequency = ?self.r82xx.crystal_frequency,
                vco_power_ref = ?self.r82xx.model.vco_power_ref(),
                "No PLL divider config found"
            );

            NoPllConfig {
                lo_frequency,
                sel_div,
                vco_frequency,
                crystal_frequency: self.r82xx.crystal_frequency,
                vco_power_ref: self.r82xx.model.vco_power_ref(),
            }
        })?;

        tracing::debug!(?pll_divider);
        self.registers.set_n_i2c(pll_divider.n_i2c);
        self.registers.set_s_i2c(pll_divider.s_i2c);
        self.registers.set_sdm_in(pll_divider.sdm);

        Ok(())
    }

    pub fn shutdown(&mut self) {
        tracing::debug!("setting {:?} to standby", self.model());

        self.shutdown_ours();

        {
            // todo: the dongle gets funky when standby is not done right. but it seems to
            // work pretty well now. we'll leave this here for a while

            let mut any_difference = false;
            for (i, expected) in SHUTDOWN_REGITSERS.iter().copied() {
                if self.registers[i] != expected {
                    println!(
                        "Register 0x{i:02x} differs:\n  expected: 0x{:02x}\n  provided: 0x{:02x}",
                        expected, self.registers[i]
                    );
                    any_difference = true;
                }
            }

            if any_difference {
                tracing::warn!("FIXME: Shutdown incomplete. Running hard-coded shutdown sequence.");
                self.shutdown_librtlsdr();
            }
        }
    }

    #[allow(dead_code)]
    fn shutdown_librtlsdr(&mut self) {
        for (address, value) in SHUTDOWN_REGITSERS.iter().copied() {
            self.registers[address] = value;
        }
    }

    #[allow(dead_code)]
    fn shutdown_ours(&mut self) {
        // 0x05
        self.registers.set_pwd_lt(true); // turn off loop-through
        self.registers.set_unk_cable_1_in(false); // disable cable_1 input
        self.registers.set_pwd_lna1(true); // turn off lna 1
        self.registers.set_lna_gain_mode(false); // why does librtlsdr set this to auto?
        self.registers.set_lna_gain(0); // set lna gain to min

        // 0x06
        self.registers.set_pwd_pdet1(true); // turn off pdet1
        self.registers.set_pwd_pdet3(false); // turn off pdet3
        self.registers.set_unk_cable_2_in(false); // disable cable_2 input
        self.registers.set_pw_lna(0b001); // don't know why librtlsdr sets this to 0b001, instead of 0b111(min)

        // 0x07
        self.registers.set_pwd_mix(false); // turn mixer off
        self.registers.set_pw0_mix(true); // mixer low power setting
        self.registers.set_mixgain_mode(true); // why does librtlsdr this to auto?
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
        self.registers.set_unk_filt_q(true); // librtlsdr sets this on standby
        self.registers.set_filt_code(0b0110); // librtlsdr sets this on standby

        // 0x0c
        self.registers.set_pwd_vga(false); // turn off vga
        self.registers.set_unk_pw0_vga(true); // low power setting
        self.registers.set_unk_adc_enable(false); // enable ADC, why enable?
        self.registers.set_vga_mode(true); // librtlsdr sets this on standby - sets VGA to be controlled by VAGC pin, so the VGA code shouldn't matter.
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
        self.registers.set_open_d(false);

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

const SHUTDOWN_REGITSERS: &[(u8, u8)] = &[
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

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        if self.warn_on_uncomitted_drop {
            let modified = self.registers.modified();
            if modified.any() {
                tracing::warn!(?modified, "dropping Transaction with modified registers");
            }
        }
    }
}

impl Tuner for R82xx {
    type Error = Error;

    fn name(&self) -> &str {
        self.model.name()
    }

    async fn set_bandwidth<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        bandwidth: f32,
    ) -> Result<(), Self::Error> {
        let mut transaction = self.begin_transaction(rtl2832u);
        transaction.set_bandwidth(bandwidth);
        transaction.commit().await
    }

    async fn set_center_frequency<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        center_frequency: f32,
    ) -> Result<(), Self::Error> {
        let mut transaction = self.begin_transaction(rtl2832u);
        transaction.set_center_frequency(center_frequency)?;
        transaction.commit().await
    }

    fn if_setting(&self) -> IfSetting {
        IfSetting::If {
            frequency: self.if_frequency,
            invert_spectrum: true,
        }
    }

    async fn shutdown<'a>(&mut self, rtl2832u: &'a mut Rtl2832u) -> Result<(), Self::Error> {
        let mut transaction = self.begin_transaction(rtl2832u);
        transaction.shutdown();
        transaction.commit().await
    }
}

/*
#[test]
fn sel_div() {
    // slightly modified code from librtlsdr for sel_div selection.

    // is their code for selecting sel_div wrong? the sel_div bits are assigned a
    // bit out of order, so 0b11 would mean no divider, according to datasheet. but
    // this doesn't seem to take this into account.

    let mut mix_div = 2;

    let vco_min: f32 = 1770000000.0;
    let vco_max = vco_min * 2.0;

    while mix_div <= 64 {
        let mut div_buf = mix_div;
        let mut div_num = 0;
        while div_buf > 2 {
            div_buf = div_buf >> 1;
            div_num += 1;
        }

        let f_min = vco_min / mix_div as f32;
        let f_max = vco_max / mix_div as f32;

        println!(
            "f={}..{} (MHz), mix_div={mix_div}, div_num={div_num:03b}",
            f_min / 1000000.0,
            f_max / 1000000.0
        );

        let our = find_sel_div(0.5 * (f_min + f_max));
        println!("our: sel_div={:03b}, f_div={}", our.0, our.1);

        mix_div = mix_div << 1;
    }

    fn find_sel_div(frequency: f32) -> (u8, f32) {
        let vco_min: f32 = 1770000000.0;

        let div = (vco_min / frequency).log2().floor().clamp(0.0, 5.0);
        let sel_div = div as u8;
        let f_div = 2.0f32.powi(i32::from(sel_div) + 1);
        (sel_div, f_div)
    }
}

#[test]
fn test_set_pll() {
    fn set_pll(frequency: f32) {
        let crystal_frequency = BLOG_CRYSTAL_FREQ as f32;
        let vco_power_ref = 1;

        let sel_div = SelDiv::from_frequency(frequency);

        // todo: librtlsdr adjusts the sel_div value using vco_fine_tune, though we
        // haven't actually observed this taking effect (we tuned a bit while logging
        // some values in r82xx_set_pll).

        //self.registers.set_sel_div(sel_div.register_value());

        // the VCO frequency we want
        // vco_freq = (uint64_t)freq * (uint64_t)mix_div;
        let vco_frequency = frequency * sel_div.effective_divider();

        dbg!(vco_frequency, crystal_frequency);

        // librtlsdr:
        //
        // uint32_t vco_fra;	/* VCO contribution by SDM (kHz) */
        // pll_ref = priv->cfg->xtal;
        // nint = vco_freq / (2 * pll_ref);
        // vco_fra = (vco_freq - 2 * pll_ref * nint) / 1000;
        // ni = (nint - 13) / 4;
        // si = nint - 4 * ni - 13;
        //
        // datasheet:
        //
        // SI2C: 2 bits
        // Ni2C: 6 bits
        //
        // Nint = 4*Ni2c+Si2c+13
        // Ndiv = (Nint + Nfra)*2
        // Nfra = SDM_IN[16] * 2^-1 + SDM_IN[15] * 2^-2 + ... + SDM_IN[2]* 2 ^-15 +
        // SDM_IN[1] * 2^-16

        let n_div = 0.5 * vco_frequency / crystal_frequency;
        dbg!(n_div);

        let n_int = n_div.floor() as u8;
        let n_fra = (n_div.fract() * 65536.0) as u16;

        if n_int > (128 / vco_power_ref) - 1 {
            todo!("return error: no valid PLL values for frequency: {frequency}");
        }

        let n_i2c = (n_int - 13) / 4;
        let s_i2c = n_int - 4 * n_i2c - 13;
    }

    set_pll(101625000.0);
}
 */
