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
        min_bandwidth: 7000000.0,
        max_bandwidth: 8000000.0,
        if_filter: IfFilterSetting {
            low_q: true,
            bw_1_7mhz: false,
            filt_bw: 0,
            hpf: 11,
            center_frequency: 4570000.0,
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
            center_frequency: 4570000.0,
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
            center_frequency: 3570000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 2050000.0,
        max_bandwidth: 2430000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 2,
            hpf: 15,
            center_frequency: 1640000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 1700000.0,
        max_bandwidth: 2050000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 1,
            hpf: 12,
            center_frequency: 1750000.0,
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
            center_frequency: 1450000.0,
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
            center_frequency: 1500000.0,
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
            center_frequency: 1525000.0,
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
            center_frequency: 1575000.0,
        },
    },
    BandwidthSetting {
        min_bandwidth: 700000.0,
        max_bandwidth: 1200000.0,
        if_filter: IfFilterSetting {
            low_q: false,
            bw_1_7mhz: true,
            filt_bw: 3,
            hpf: 10,
            center_frequency: 1850000.0,
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
            center_frequency: 1950000.0,
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
            center_frequency: 2025000.0,
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
            center_frequency: 2075000.0,
        },
    },
];

pub fn bandwidth_setting(bandwidth: f32) -> &'static BandwidthSetting {
    PRESET_BANDWIDTH_SETTINGS
        .iter()
        .find(|setting| setting.min_bandwidth < bandwidth)
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
        end_frequency: 0.0,
        tracking_filter: TrackingFilterSetting {
            open_d: OpenD::HighZ,
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
            open_d: OpenD::HighZ,
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
            open_d: OpenD::HighZ,
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
            open_d: OpenD::HighZ,
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
            open_d: OpenD::HighZ,
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
            open_d: OpenD::HighZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
            open_d: OpenD::LowZ,
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
