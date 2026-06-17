use crate::tuner::r82xx::{
    CrystalCapacitor,
    IfFilterSetting,
    LnaGainCode,
    MixGainCode,
    OpenD,
    RfFilt,
    RfMux,
    TrackingFilterSetting,
    VgaGainCode,
};

#[derive(Clone, Copy, Debug)]
pub struct BandwidthSetting {
    pub min_bandwidth: f32,
    pub max_bandwidth: f32,
    pub if_filter: IfFilterSetting,
}

pub const PRESET_BANDWIDTH_SETTINGS: &[BandwidthSetting] = &[
    BandwidthSetting {
        min_bandwidth: 0.0,
        max_bandwidth: 350000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 6,
            if_frequency: 2125000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 350000.0,
        max_bandwidth: 450000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 7,
            if_frequency: 2075000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 450000.0,
        max_bandwidth: 550000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 8,
            if_frequency: 2025000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 550000.0,
        max_bandwidth: 700000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 9,
            if_frequency: 1950000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 700000.0,
        max_bandwidth: 900000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 10,
            if_frequency: 1850000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 900000.0,
        max_bandwidth: 1200000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 11,
            if_frequency: 1700000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 1200000.0,
        max_bandwidth: 1450000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 12,
            if_frequency: 1575000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 1450000.0,
        max_bandwidth: 1550000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 13,
            if_frequency: 1525000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 1550000.0,
        max_bandwidth: 1600000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 14,
            if_frequency: 1500000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 1600000.0,
        max_bandwidth: 1700000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 15,
            if_frequency: 1450000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 1700000.0,
        max_bandwidth: 1800000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 1,
            hpf: 12,
            if_frequency: 1750000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 1800000.0,
        max_bandwidth: 1900000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 1,
            hpf: 13,
            if_frequency: 1700000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 1900000.0,
        max_bandwidth: 1950000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 1,
            hpf: 14,
            if_frequency: 1675000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 1950000.0,
        max_bandwidth: 2050000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 1,
            hpf: 15,
            if_frequency: 1625000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 2050000.0,
        max_bandwidth: 2080000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 2,
            hpf: 15,
            if_frequency: 1640000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 2080000.0,
        max_bandwidth: 2180000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 0,
            hpf: 12,
            if_frequency: 1940000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 2180000.0,
        max_bandwidth: 2280000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 0,
            hpf: 13,
            if_frequency: 1890000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 2280000.0,
        max_bandwidth: 2330000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 0,
            hpf: 14,
            if_frequency: 1865000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 2330000.0,
        max_bandwidth: 2430000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 0,
            hpf: 15,
            if_frequency: 1815000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 2430000.0,
        max_bandwidth: 6000000.0,
        if_filter: IfFilterSetting {
            low_q: true,
            bw_1_7mhz: false,
            filt_bw: 3,
            hpf: 11,
            if_frequency: 3570000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 6000000.0,
        max_bandwidth: 7000000.0,
        if_filter: IfFilterSetting {
            low_q: true,
            bw_1_7mhz: false,
            filt_bw: 1,
            hpf: 10,
            if_frequency: 4570000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 7000000.0,
        max_bandwidth: 8000000.0,
        if_filter: IfFilterSetting {
            low_q: true,
            bw_1_7mhz: false,
            filt_bw: 0,
            hpf: 11,
            if_frequency: 4570000.0,
        },
    },
];

pub fn bandwidth_setting(bandwidth: f32) -> &'static BandwidthSetting {
    // todo: do binary search instead

    PRESET_BANDWIDTH_SETTINGS
        .iter()
        .find(|setting| bandwidth < setting.max_bandwidth)
        .unwrap_or_else(|| PRESET_BANDWIDTH_SETTINGS.last().unwrap())
}

#[derive(Clone, Copy, Debug)]
pub struct FrequencySetting {
    pub start_frequency: f32,
    pub end_frequency: f32,

    /// Settings for the tracking filter
    ///
    /// If you don't want to use the tracking filter, set
    /// [`TrackingFilterSetting::rf_mux`] to [`RfMux::Bypass`]. The other
    /// values can be ignored then, but we left them here so they
    /// can still be set for experimentation.
    pub tracking_filter: TrackingFilterSetting,

    /// Max crystal capacitor value to use with this setting.
    ///
    /// # Note
    ///
    /// We think `r82xx_set_mux` basically just selects the minimum of the
    /// selected crystal capacitor value and this setting. Though we're not
    /// 100% sure. librtsldr selects 0pF, high for the crystal
    /// settings, so the effective settings will be always that.
    pub crystal_capacitor: CrystalCapacitor,
}

pub const PRESET_FREQUENCY_SETTINGS: &[FrequencySetting] = &[
    FrequencySetting {
        start_frequency: 0.0,
        end_frequency: 50_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::LowZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 15,
            tf_nch: 13,
        },
        crystal_capacitor: CrystalCapacitor::P20,
    },
    FrequencySetting {
        start_frequency: 50_000_000.0,
        end_frequency: 55_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::LowZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 14,
            tf_nch: 11,
        },
        crystal_capacitor: CrystalCapacitor::P20,
    },
    FrequencySetting {
        start_frequency: 55_000_000.0,
        end_frequency: 60_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::LowZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 11,
            tf_nch: 8,
        },
        crystal_capacitor: CrystalCapacitor::P20,
    },
    FrequencySetting {
        start_frequency: 60_000_000.0,
        end_frequency: 65_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::LowZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 11,
            tf_nch: 7,
        },
        crystal_capacitor: CrystalCapacitor::P20,
    },
    FrequencySetting {
        start_frequency: 65_000_000.0,
        end_frequency: 70_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::LowZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 9,
            tf_nch: 6,
        },
        crystal_capacitor: CrystalCapacitor::P20,
    },
    FrequencySetting {
        start_frequency: 70_000_000.0,
        end_frequency: 75_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::LowZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 8,
            tf_nch: 5,
        },
        crystal_capacitor: CrystalCapacitor::P20,
    },
    FrequencySetting {
        start_frequency: 75_000_000.0,
        end_frequency: 80_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 4,
            tf_nch: 4,
        },
        crystal_capacitor: CrystalCapacitor::P20,
    },
    FrequencySetting {
        start_frequency: 80_000_000.0,
        end_frequency: 90_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 4,
            tf_nch: 4,
        },
        crystal_capacitor: CrystalCapacitor::P20,
    },
    FrequencySetting {
        start_frequency: 90_000_000.0,
        end_frequency: 100_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 4,
            tf_nch: 3,
        },
        crystal_capacitor: CrystalCapacitor::P10,
    },
    FrequencySetting {
        start_frequency: 100_000_000.0,
        end_frequency: 110_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 4,
            tf_nch: 3,
        },
        crystal_capacitor: CrystalCapacitor::P10,
    },
    FrequencySetting {
        start_frequency: 110_000_000.0,
        end_frequency: 120_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 4,
            tf_nch: 2,
        },
        crystal_capacitor: CrystalCapacitor::P10,
    },
    FrequencySetting {
        start_frequency: 120_000_000.0,
        end_frequency: 140_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 4,
            tf_nch: 2,
        },
        crystal_capacitor: CrystalCapacitor::P10,
    },
    FrequencySetting {
        start_frequency: 140_000_000.0,
        end_frequency: 180_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 4,
            tf_nch: 1,
        },
        crystal_capacitor: CrystalCapacitor::P10,
    },
    FrequencySetting {
        start_frequency: 180_000_000.0,
        end_frequency: 220_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 3,
            tf_nch: 1,
        },
        crystal_capacitor: CrystalCapacitor::P0,
    },
    FrequencySetting {
        start_frequency: 220_000_000.0,
        end_frequency: 250_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 3,
            tf_nch: 1,
        },
        crystal_capacitor: CrystalCapacitor::P0,
    },
    FrequencySetting {
        start_frequency: 250_000_000.0,
        end_frequency: 280_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 1,
            tf_nch: 1,
        },
        crystal_capacitor: CrystalCapacitor::P0,
    },
    FrequencySetting {
        start_frequency: 280_000_000.0,
        end_frequency: 310_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::TrackingFilter,
            rf_filt: RfFilt::Low,
            tf_lp: 0,
            tf_nch: 0,
        },
        crystal_capacitor: CrystalCapacitor::P0,
    },
    FrequencySetting {
        start_frequency: 310_000_000.0,
        end_frequency: 450_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::Bypass,
            rf_filt: RfFilt::Medium,
            tf_lp: 0,
            tf_nch: 0,
        },
        crystal_capacitor: CrystalCapacitor::P0,
    },
    FrequencySetting {
        start_frequency: 450_000_000.0,
        end_frequency: 588_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::Bypass,
            rf_filt: RfFilt::Medium,
            tf_lp: 0,
            tf_nch: 0,
        },
        crystal_capacitor: CrystalCapacitor::P0,
    },
    FrequencySetting {
        start_frequency: 588_000_000.0,
        end_frequency: 650_000_000.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::Bypass,
            rf_filt: RfFilt::Highest,
            tf_lp: 0,
            tf_nch: 0,
        },
        crystal_capacitor: CrystalCapacitor::P0,
    },
    FrequencySetting {
        start_frequency: 650_000_000.0,
        end_frequency: f32::INFINITY,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::Bypass,
            rf_filt: RfFilt::Highest,
            tf_lp: 0,
            tf_nch: 0,
        },
        crystal_capacitor: CrystalCapacitor::P0,
    },
];

pub fn frequency_setting(frequency: f32) -> &'static FrequencySetting {
    // todo: do binary search instead

    PRESET_FREQUENCY_SETTINGS
        .iter()
        .find(|setting| frequency < setting.end_frequency)
        .unwrap_or_else(|| PRESET_FREQUENCY_SETTINGS.last().unwrap())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GainSetting {
    pub lna: LnaGainCode,
    pub mix: MixGainCode,
    pub vga: VgaGainCode,
}

impl GainSetting {
    pub fn as_db(&self) -> f32 {
        self.lna.as_db() + self.mix.as_db() + self.vga.as_db()
    }
}

/// Presets for LNA, mixer, and VGA gain settings
pub const GAIN_SETTINGS: [GainSetting; 30] = {
    const fn gs(lna: u8, mix: u8, vga: u8) -> GainSetting {
        GainSetting {
            lna: LnaGainCode::from_code_unchecked(lna),
            mix: MixGainCode::from_code_unchecked(mix),
            vga: VgaGainCode::from_code_unchecked(vga),
        }
    }
    [
        gs(0, 0, 8),   // 16.3
        gs(0, 1, 8),   // 16.8
        gs(1, 1, 8),   // 17.7
        gs(1, 2, 8),   // 18.7
        gs(2, 2, 8),   // 20.0
        gs(2, 3, 8),   // 21.0
        gs(3, 3, 8),   // 25.0
        gs(3, 4, 8),   // 26.9
        gs(4, 4, 8),   // 30.7
        gs(4, 5, 8),   // 31.6
        gs(5, 5, 8),   // 32.9
        gs(5, 6, 8),   // 33.9
        gs(6, 6, 8),   // 37.0
        gs(7, 6, 8),   // 39.2
        gs(7, 7, 8),   // 41.7
        gs(7, 8, 8),   // 43.4
        gs(8, 8, 8),   // 46.0
        gs(8, 9, 8),   // 47.0
        gs(9, 9, 8),   // 50.1
        gs(9, 10, 8),  // 50.9
        gs(10, 10, 8), // 53.5
        gs(11, 10, 8), // 54.9
        gs(11, 11, 8), // 56.5
        gs(11, 12, 8), // 57.8
        gs(12, 12, 8), // 59.7
        gs(13, 12, 8), // 60.2
        gs(13, 13, 8), // 60.8
        gs(13, 14, 8), // 61.1
        gs(14, 15, 8), // 63.8
        gs(15, 15, 8), // 65.1
    ]
};

pub const GAIN_VALUES: [f32; 30] = [
    16.3, 16.8, 17.7, 18.7, 20.0, 21.0, 25.0, 26.9, 30.7, 31.6, 32.9, 33.9, 37.0, 39.2, 41.7, 43.4,
    46.0, 47.0, 50.1, 50.9, 53.5, 54.9, 56.5, 57.8, 59.7, 60.2, 60.8, 61.1, 63.8, 65.1,
];

/*
#[test]
fn filter_configs_bruteforce() {
    let mut current_config: Option<((u8, u8, u32), u32)> = None;
    let mut configs = vec![];

    let mut emit = |config, min_bandwidth, max_bandwidth| {
        let (reg_0a, reg_0b, if_frequency) = config;

        let low_q = reg_0a != 0;
        let bw_1_7mhz = reg_0b & 0x80 != 0;
        let filt_bw = (reg_0b >> 5) & 3u8;
        let hpf = reg_0b & 0xf;

        let config = BandwidthSetting {
            min_bandwidth: min_bandwidth as f32,
            max_bandwidth: max_bandwidth as f32,
            if_filter: IfFilterSetting {
                low_q,
                bw_1_7mhz,
                filt_bw,
                hpf,
                if_frequency: if_frequency as f32,
            },
        };

        println!("{config:?}");

        configs.push(config);
    };

    for bandwidth in 0..=8000000 {
        let config = filter_config(bandwidth);

        if let Some((current, min_bandwidth)) = current_config
            && config != current
        {
            emit(current, min_bandwidth.saturating_sub(1), bandwidth - 1);
            current_config = None;
        }

        if current_config.is_none() {
            current_config = Some((config, bandwidth));
        }
    }

    if let Some((config, min_bandwidth)) = current_config.take() {
        emit(config, min_bandwidth - 1, 8000000);
    }
}

#[allow(dead_code)]
fn filter_config(mut bw: u32) -> (u8, u8, u32) {
    const R82XX_IF_LOW_PASS_BW_TABLE: [u32; 10] = [
        1700000, 1600000, 1550000, 1450000, 1200000, 900000, 700000, 550000, 450000, 350000,
    ];
    const FILT_HP_BW1: u32 = 350000;
    const FILT_HP_BW2: u32 = 380000;

    let reg_0a: u8;
    let mut reg_0b: u8;
    let mut int_freq;
    let mut real_bw = 0;

    if bw > 7000000 {
        // BW: 8 MHz
        reg_0a = 0x10;
        reg_0b = 0x0b;
        int_freq = 4570000;
    }
    else if bw > 6000000 {
        // BW: 7 MHz
        reg_0a = 0x10;
        reg_0b = 0x2a;
        int_freq = 4570000;
    }
    else if bw > R82XX_IF_LOW_PASS_BW_TABLE[0] + FILT_HP_BW1 + FILT_HP_BW2 {
        // BW: 6 MHz
        reg_0a = 0x10;
        reg_0b = 0x6b;
        int_freq = 3570000;
    }
    else {
        reg_0a = 0x00;
        reg_0b = 0x80;
        int_freq = 2300000;

        if bw > R82XX_IF_LOW_PASS_BW_TABLE[0] + FILT_HP_BW1 {
            bw -= FILT_HP_BW2;
            int_freq += FILT_HP_BW2;
            real_bw += FILT_HP_BW2;
        }
        else {
            reg_0b |= 0x20;
        }

        if bw > R82XX_IF_LOW_PASS_BW_TABLE[0] {
            bw -= FILT_HP_BW1;
            int_freq += FILT_HP_BW1;
            real_bw += FILT_HP_BW1;
        }
        else {
            reg_0b |= 0x40;
        }

        // find low-pass filter
        let mut i: u8 = 0;
        while i < 10 {
            if bw > R82XX_IF_LOW_PASS_BW_TABLE[usize::from(i)] {
                break;
            }
            i += 1;
        }
        i -= 1;
        reg_0b |= 15 - i;
        real_bw += R82XX_IF_LOW_PASS_BW_TABLE[usize::from(i)];

        int_freq -= real_bw / 2;
    }

    (reg_0a, reg_0b, int_freq)
}


#[test]
fn make_gain_settings() {
    let vga = VgaGainCode::from_code(0x08).unwrap();

    let mut presets = vec![];

    for lna_code in 0..16 {
        for mix_code in 0..16 {
            presets.push(GainSetting {
                lna: LnaGainCode::from_code(lna_code).unwrap(),
                mix: MixGainCode::from_code(mix_code).unwrap(),
                vga,
            });
        }
    }

    // sort by gain in dB
    presets.sort_by(|a, b| a.as_db().partial_cmp(&b.as_db()).unwrap());

    // only keep presets where lna and mix gain codes differ at most by 1
    presets
        .retain(|preset| (i32::from(preset.lna.code()) - i32::from(preset.mix.code())).abs() <= 1);

    // remove any presets that would decrease either lna or mix gain
    {
        let mut previous_lna_gain = LnaGainCode::ZERO;
        let mut previous_mix_gain = MixGainCode::ZERO;

        presets.retain(|preset| {
            let keep = preset.lna >= previous_lna_gain && preset.mix >= previous_mix_gain;
            if keep {
                previous_lna_gain = preset.lna;
                previous_mix_gain = preset.mix;
            }
            keep
        });
    }

    // remove any presets that would result in the same gain in dB
    presets.dedup_by(|a, b| (a.as_db() * 10.0).round() == (b.as_db() * 10.0).round());

    let num_presets = presets.len();

    println!("/// Presets for LNA, mixer, and VGA gain settings");
    println!("pub const GAIN_SETTINGS: [GainSetting; {num_presets}] = {{");
    println!("    const fn gs(lna: u8, mix: u8, vga: u8) -> GainSetting {{");
    println!(
        "        GainSetting {{ lna: LnaGainCode::from_code_unchecked(lna), mix: MixGainCode::from_code_unchecked(mix), vga: VgaGainCode::from_code_unchecked(vga) }}"
    );
    println!("    }}");
    println!("    [");
    for preset in &presets {
        println!(
            "        gs({:?}, {:?}, {:?}), // {:.1}",
            preset.lna.code(),
            preset.mix.code(),
            preset.vga.code(),
            preset.as_db(),
        );
    }
    println!("    ]");
    println!("}};");

    println!("pub const GAIN_VALUES: [f32; {num_presets}] = [");
    for gain in &presets {
        println!("  {:.1},", gain.as_db());
    }
    println!("];");
}
*/

#[derive(Clone, Copy, Debug)]
pub struct NotchBand {
    pub start_frequency: f32,
    pub end_frequency: f32,
}

pub const NOTCH_BANDS: &[NotchBand] = &[
    NotchBand {
        start_frequency: 0.0,
        end_frequency: 2200000.0,
    },
    NotchBand {
        start_frequency: 85000000.0,
        end_frequency: 112000000.0,
    },
    NotchBand {
        start_frequency: 172000000.0,
        end_frequency: 242000000.0,
    },
];

pub fn is_in_notch_band(frequency: f32) -> bool {
    NOTCH_BANDS.iter().any(|notch_band| {
        notch_band.start_frequency <= frequency && frequency <= notch_band.end_frequency
    })
}

#[cfg(test)]
mod tests {
    use crate::tuner::r82xx::{
        CrystalCapacitor,
        OpenD,
        RfFilt,
        RfMux,
        TrackingFilterSetting,
        preset::{
            GAIN_SETTINGS,
            GAIN_VALUES,
            PRESET_BANDWIDTH_SETTINGS,
            PRESET_FREQUENCY_SETTINGS,
            bandwidth_setting,
            frequency_setting,
        },
    };

    #[test]
    fn test_bandwidth_config() {
        // $ ./src/rtl_tcp -f 144000000 -s 2400000
        // r82xx_set_freq: freq=144000000, upconvert_freq=144000000, lo_freq=145815000,
        // int_freq=1815000

        assert_eq!(
            bandwidth_setting(2400000.0).if_filter.if_frequency,
            1815000.0
        );
    }

    #[test]
    fn test_tracking_filter() {
        // $ ./src/rtl_tcp -f 144000000 -s 2400000
        // r82xx_set_mux: freq=30, open_d=08, rf_mux_ploy=02, tf_c=df, cap=00
        // r82xx_set_mux: freq=145, open_d=00, rf_mux_ploy=02, tf_c=14, cap=00

        // note r82xx_set_mux takes min of the cap value in the array and one selected
        // at init (P0, high), so it'll basically always use cap=00, but we only get the
        // preset value here, so we can't test this.
        let selected_cap = CrystalCapacitor::P0;

        let setting = frequency_setting(30000000.0);
        assert_eq!(
            setting.tracking_filter,
            TrackingFilterSetting {
                open_d: OpenD::LowZ,
                rf_mux: RfMux::TrackingFilter,
                rf_filt: RfFilt::Low,
                tf_lp: 0xf,
                tf_nch: 0xd,
            }
        );
        assert_eq!(
            setting.crystal_capacitor.min(selected_cap),
            CrystalCapacitor::P0
        );

        let setting = frequency_setting(145000000.0);
        assert_eq!(
            setting.tracking_filter,
            TrackingFilterSetting {
                open_d: OpenD::HighZ,
                rf_mux: RfMux::TrackingFilter,
                rf_filt: RfFilt::Low,
                tf_lp: 0x4,
                tf_nch: 0x1,
            }
        );
        assert_eq!(
            setting.crystal_capacitor.min(selected_cap),
            CrystalCapacitor::P0
        );
    }

    #[test]
    fn presets_are_sorted() {
        assert!(PRESET_FREQUENCY_SETTINGS.len() > 0);

        assert!(
            PRESET_FREQUENCY_SETTINGS[0].start_frequency
                < PRESET_FREQUENCY_SETTINGS[0].end_frequency
        );

        for i in 1..PRESET_FREQUENCY_SETTINGS.len() {
            assert!(
                PRESET_FREQUENCY_SETTINGS[i].start_frequency
                    < PRESET_FREQUENCY_SETTINGS[i].end_frequency
            );
            assert!(
                PRESET_FREQUENCY_SETTINGS[i - 1].end_frequency
                    <= PRESET_FREQUENCY_SETTINGS[i].start_frequency
            );
        }

        assert!(PRESET_BANDWIDTH_SETTINGS.len() > 0);

        assert!(
            PRESET_BANDWIDTH_SETTINGS[0].min_bandwidth < PRESET_BANDWIDTH_SETTINGS[0].max_bandwidth
        );

        for i in 1..PRESET_BANDWIDTH_SETTINGS.len() {
            assert!(
                PRESET_BANDWIDTH_SETTINGS[i].min_bandwidth
                    < PRESET_BANDWIDTH_SETTINGS[i].max_bandwidth
            );
            assert!(
                PRESET_BANDWIDTH_SETTINGS[i - 1].max_bandwidth
                    <= PRESET_BANDWIDTH_SETTINGS[i].min_bandwidth
            );
        }
    }

    #[test]
    fn gain_settings() {
        for (i, gain_setting) in GAIN_SETTINGS.iter().enumerate() {
            assert_eq!((gain_setting.as_db() * 10.0).round() / 10.0, GAIN_VALUES[i]);
        }

        // can't compare to librtlsdr as we use different presets.
        /*
        #[rustfmt::skip]
        pub const LIBRTLSDR_COMBINED_GAINS: [i32; 30] = [
            0, 9, 14, 27, 37, 77, 87, 125, 144, 157, 166, 197, 207, 229, 254, 280, 297, 328, 338,
            364, 372, 386, 402, 421, 434, 439, 445, 480,

            // for some mysterious reason librtlsdr skips this one
            483,

            496,
        ];

        let mut previous_mix_code: Option<MixGainCode> = None;

        let fixed_vga_code = VgaGainCode::from_code(0x08).unwrap();

        for (i, gain_setting) in GAIN_SETTINGS.iter().enumerate() {
            println!(
                "{i}: {gain_setting:?} {:.1} dB, {:.1} dB",
                gain_setting.lna.as_db() + gain_setting.mix.as_db(),
                gain_setting.as_db() - gain_setting.vga.as_db(),
            );

            assert_eq!(
                ((gain_setting.as_db() - gain_setting.vga.as_db()) * 10.0).round() as i32,
                LIBRTLSDR_COMBINED_GAINS[i]
            );

            assert_eq!(gain_setting.vga, fixed_vga_code);

            if i % 2 == 0 {
                assert_eq!(gain_setting.lna.code(), gain_setting.mix.code());

                if let Some(previous_mix_code) = previous_mix_code {
                    assert!(gain_setting.lna.code() > previous_mix_code.code());
                    assert!(gain_setting.mix.code() > previous_mix_code.code());
                }
            }
            else {
                assert!(gain_setting.lna.code() > gain_setting.mix.code());

                if let Some(previous_mix_code) = previous_mix_code {
                    assert!(gain_setting.lna.code() > previous_mix_code.code());
                    assert_eq!(gain_setting.mix.code(), previous_mix_code.code());
                }
            }

            previous_mix_code = Some(gain_setting.mix);
        }*/
    }
}
