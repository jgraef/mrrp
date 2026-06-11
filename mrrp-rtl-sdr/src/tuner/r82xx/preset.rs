use crate::tuner::r82xx::{
    CrystalCapacitor,
    IfFilterSetting,
    OpenD,
    RfFilt,
    RfMux,
    TrackingFilterSetting,
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
    PRESET_FREQUENCY_SETTINGS
        .iter()
        .find(|setting| frequency < setting.end_frequency)
        .unwrap_or_else(|| PRESET_FREQUENCY_SETTINGS.last().unwrap())
}

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
 */

#[cfg(test)]
mod tests {
    use crate::tuner::r82xx::{
        CrystalCapacitor,
        OpenD,
        RfFilt,
        RfMux,
        TrackingFilterSetting,
        preset::{
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
}
