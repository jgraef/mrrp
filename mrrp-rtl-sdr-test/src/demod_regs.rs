#[derive(Debug)]
struct DemodReg {
    pub name: &'static str,
    pub page: u8,
    pub start_address: u8,
    pub lsb: u8,
    pub msb: u8,

    pub reg_width: u8,
    pub ty: &'static str,
}

pub fn demod_regs() {
    // NOTE: this produces wrong layouts for registers that overlap and have
    // different starting addresses. it's only really. This is really only an issue
    // with `cfreq_off_ratio` and `rsamp_ratio`, so we fixed it manually.

    let mut regs = vec![];

    for line in SOURCE.lines() {
        let line = line.trim();
        if line.starts_with("//") || line.is_empty() {
            continue;
        }

        let line = line.strip_prefix("{").unwrap().strip_suffix("},").unwrap();
        let mut parts = line.split(',');

        let name = parts.next().unwrap().trim().strip_prefix("DVBT_").unwrap();
        let page = u8::from_str_radix(parts.next().unwrap().trim().strip_prefix("0x").unwrap(), 16)
            .unwrap();
        let start_address =
            u8::from_str_radix(parts.next().unwrap().trim().strip_prefix("0x").unwrap(), 16)
                .unwrap();
        let lsb: u8 = parts.next().unwrap().trim().parse().unwrap();
        let msb: u8 = parts.next().unwrap().trim().parse().unwrap();

        let ty_width = lsb - msb + 1;
        let ty = if ty_width == 1 {
            "bool"
        }
        else {
            let ty_width = ty_width.next_power_of_two().max(8);
            match ty_width {
                8 => "u8",
                16 => "u16",
                32 => "u32",
                _ => panic!("ty_width = {ty_width}"),
            }
        };

        regs.push(DemodReg {
            name,
            page,
            start_address,
            lsb,
            msb,
            reg_width: lsb.next_power_of_two().max(8),
            ty,
        })
    }

    regs.sort_by_key(|reg| (reg.page, reg.start_address, reg.lsb, reg.msb));

    let mut it = regs.iter().peekable();

    let mut names = vec![];
    let mut lines = vec![];

    while let Some(reg) = it.next() {
        names.push(reg.name);
        let name_lower = reg.name.to_lowercase();
        let bit_range = if reg.lsb != reg.msb {
            format!("{}, {}", reg.lsb, reg.msb)
        }
        else {
            format!("{}", reg.lsb)
        };
        lines.push(format!(
            "            /// {}: {}, 0x{:02x}\n            pub {}, {}, set_{}: {};",
            reg.name, reg.page, reg.start_address, reg.ty, name_lower, name_lower, bit_range,
        ));

        if let Some(next) = it.peek()
            && reg.page == next.page
            && next.start_address < reg.start_address + reg.reg_width / 8
        {
            if reg.reg_width > 8 {
                println!("            // VERIFY");
            }
            //
        }
        else {
            let merged_name = names.join("_");
            names.clear();
            println!(
                "        {merged_name}: u{} = demod({}, 0x{:02x}) {{",
                reg.reg_width, reg.page, reg.start_address,
            );

            for line in lines.drain(..) {
                println!("{line}");
            }

            println!("        }};");
        }
    }
}

static SOURCE: &str = r#"
		// Software reset register
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_SOFT_RST,						0x1,		0x1,			2,		2},

		// Tuner I2C forwording register
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_IIC_REPEAT,					0x1,		0x1,			3,		3},

		// Registers for initialization
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_TR_WAIT_MIN_8K,				0x1,		0x88,			11,		2},
		{DVBT_RSD_BER_FAIL_VAL,				0x1,		0x8f,			15,		0},
		{DVBT_EN_BK_TRK,					0x1,		0xa6,			7,		7},
		{DVBT_AD_EN_REG,					0x0,		0x8,			7,		7},
		{DVBT_AD_EN_REG1,					0x0,		0x8,			6,		6},
		{DVBT_EN_BBIN,						0x1,		0xb1,			0,		0},
		{DVBT_MGD_THD0,						0x1,		0x95,			7,		0},
		{DVBT_MGD_THD1,						0x1,		0x96,			7,		0},
		{DVBT_MGD_THD2,						0x1,		0x97,			7,		0},
		{DVBT_MGD_THD3,						0x1,		0x98,			7,		0},
		{DVBT_MGD_THD4,						0x1,		0x99,			7,		0},
		{DVBT_MGD_THD5,						0x1,		0x9a,			7,		0},
		{DVBT_MGD_THD6,						0x1,		0x9b,			7,		0},
		{DVBT_MGD_THD7,						0x1,		0x9c,			7,		0},
		{DVBT_EN_CACQ_NOTCH,				0x1,		0x61,			4,		4},
		{DVBT_AD_AV_REF,					0x0,		0x9,			6,		0},
		{DVBT_REG_PI,						0x0,		0xa,			2,		0},
		{DVBT_PIP_ON,						0x0,		0x21,			3,		3},
		{DVBT_SCALE1_B92,					0x2,		0x92,			7,		0},
		{DVBT_SCALE1_B93,					0x2,		0x93,			7,		0},
		{DVBT_SCALE1_BA7,					0x2,		0xa7,			7,		0},
		{DVBT_SCALE1_BA9,					0x2,		0xa9,			7,		0},
		{DVBT_SCALE1_BAA,					0x2,		0xaa,			7,		0},
		{DVBT_SCALE1_BAB,					0x2,		0xab,			7,		0},
		{DVBT_SCALE1_BAC,					0x2,		0xac,			7,		0},
		{DVBT_SCALE1_BB0,					0x2,		0xb0,			7,		0},
		{DVBT_SCALE1_BB1,					0x2,		0xb1,			7,		0},
		{DVBT_KB_P1,						0x1,		0x64,			3,		1},
		{DVBT_KB_P2,						0x1,		0x64,			6,		4},
		{DVBT_KB_P3,						0x1,		0x65,			2,		0},
		{DVBT_OPT_ADC_IQ,					0x0,		0x6,			5,		4},
		{DVBT_AD_AVI,						0x0,		0x9,			1,		0},
		{DVBT_AD_AVQ,						0x0,		0x9,			3,		2},
		{DVBT_K1_CR_STEP12,					0x2,		0xad,			9,		4},

		// Registers for initialization according to mode
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_TRK_KS_P2,					0x1,		0x6f,			2,		0},
		{DVBT_TRK_KS_I2,					0x1,		0x70,			5,		3},
		{DVBT_TR_THD_SET2,					0x1,		0x72,			3,		0},
		{DVBT_TRK_KC_P2,					0x1,		0x73,			5,		3},
		{DVBT_TRK_KC_I2,					0x1,		0x75,			2,		0},
		{DVBT_CR_THD_SET2,					0x1,		0x76,			7,		6},

		// Registers for IF setting
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_PSET_IFFREQ,					0x1,		0x19,			21,		0},
		{DVBT_SPEC_INV,						0x1,		0x15,			0,		0},

		// Registers for bandwidth programming
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_RSAMP_RATIO,					0x1,		0x9f,			27,		2},
		{DVBT_CFREQ_OFF_RATIO,				0x1,		0x9d,			23,		4},

		// FSM stage register
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_FSM_STAGE,					0x3,		0x51,			6,		3},

		// TPS content registers
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_RX_CONSTEL,					0x3,		0x3c,			3,		2},
		{DVBT_RX_HIER,						0x3,		0x3c,			6,		4},
		{DVBT_RX_C_RATE_LP,					0x3,		0x3d,			2,		0},
		{DVBT_RX_C_RATE_HP,					0x3,		0x3d,			5,		3},
		{DVBT_GI_IDX,						0x3,		0x51,			1,		0},
		{DVBT_FFT_MODE_IDX,					0x3,		0x51,			2,		2},

		// Performance measurement registers
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_RSD_BER_EST,					0x3,		0x4e,			15,		0},
		{DVBT_CE_EST_EVM,					0x4,		0xc,			15,		0},

		// AGC registers
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_RF_AGC_VAL,					0x3,		0x5b,			13,		0},
		{DVBT_IF_AGC_VAL,					0x3,		0x59,			13,		0},
		{DVBT_DAGC_VAL,						0x3,		0x5,			7,		0},

		// TR offset and CR offset registers
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_SFREQ_OFF,					0x3,		0x18,			13,		0},
		{DVBT_CFREQ_OFF,					0x3,		0x5f,			17,		0},

		// AGC relative registers
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_POLAR_RF_AGC,					0x0,		0xe,			1,		1},
		{DVBT_POLAR_IF_AGC,					0x0,		0xe,			0,		0},
		{DVBT_AAGC_HOLD,					0x1,		0x4,			5,		5},
		{DVBT_EN_RF_AGC,					0x1,		0x4,			6,		6},
		{DVBT_EN_IF_AGC,					0x1,		0x4,			7,		7},
		{DVBT_IF_AGC_MIN,					0x1,		0x8,			7,		0},
		{DVBT_IF_AGC_MAX,					0x1,		0x9,			7,		0},
		{DVBT_RF_AGC_MIN,					0x1,		0xa,			7,		0},
		{DVBT_RF_AGC_MAX,					0x1,		0xb,			7,		0},
		{DVBT_IF_AGC_MAN,					0x1,		0xc,			6,		6},
		{DVBT_IF_AGC_MAN_VAL,				0x1,		0xc,			13,		0},
		{DVBT_RF_AGC_MAN,					0x1,		0xe,			6,		6},
		{DVBT_RF_AGC_MAN_VAL,				0x1,		0xe,			13,		0},
		{DVBT_DAGC_TRG_VAL,					0x1,		0x12,			7,		0},
		{DVBT_AGC_TARG_VAL_0,				0x1,		0x2,			0,		0},
		{DVBT_AGC_TARG_VAL_8_1,				0x1,		0x3,			7,		0},
		{DVBT_AAGC_LOOP_GAIN,				0x1,		0xc7,			5,		1},
		{DVBT_LOOP_GAIN2_3_0,				0x1,		0x4,			4,		1},
		{DVBT_LOOP_GAIN2_4,					0x1,		0x5,			7,		7},
		{DVBT_LOOP_GAIN3,					0x1,		0xc8,			4,		0},
		{DVBT_VTOP1,						0x1,		0x6,			5,		0},
		{DVBT_VTOP2,						0x1,		0xc9,			5,		0},
		{DVBT_VTOP3,						0x1,		0xca,			5,		0},
		{DVBT_KRF1,							0x1,		0xcb,			7,		0},
		{DVBT_KRF2,							0x1,		0x7,			7,		0},
		{DVBT_KRF3,							0x1,		0xcd,			7,		0},
		{DVBT_KRF4,							0x1,		0xce,			7,		0},
		{DVBT_EN_GI_PGA,					0x1,		0xe5,			0,		0},
		{DVBT_THD_LOCK_UP,					0x1,		0xd9,			8,		0},
		{DVBT_THD_LOCK_DW,					0x1,		0xdb,			8,		0},
		{DVBT_THD_UP1,						0x1,		0xdd,			7,		0},
		{DVBT_THD_DW1,						0x1,		0xde,			7,		0},
		{DVBT_INTER_CNT_LEN,				0x1,		0xd8,			3,		0},
		{DVBT_GI_PGA_STATE,					0x1,		0xe6,			3,		3},
		{DVBT_EN_AGC_PGA,					0x1,		0xd7,			0,		0},

		// TS interface registers
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_CKOUTPAR,						0x1,		0x7b,			5,		5},
		{DVBT_CKOUT_PWR,					0x1,		0x7b,			6,		6},
		{DVBT_SYNC_DUR,						0x1,		0x7b,			7,		7},
		{DVBT_ERR_DUR,						0x1,		0x7c,			0,		0},
		{DVBT_SYNC_LVL,						0x1,		0x7c,			1,		1},
		{DVBT_ERR_LVL,						0x1,		0x7c,			2,		2},
		{DVBT_VAL_LVL,						0x1,		0x7c,			3,		3},
		{DVBT_SERIAL,						0x1,		0x7c,			4,		4},
		{DVBT_SER_LSB,						0x1,		0x7c,			5,		5},
		{DVBT_CDIV_PH0,						0x1,		0x7d,			3,		0},
		{DVBT_CDIV_PH1,						0x1,		0x7d,			7,		4},
		{DVBT_MPEG_IO_OPT_2_2,				0x0,		0x6,			7,		7},
		{DVBT_MPEG_IO_OPT_1_0,				0x0,		0x7,			7,		6},
		{DVBT_CKOUTPAR_PIP,					0x0,		0xb7,			4,		4},
		{DVBT_CKOUT_PWR_PIP,				0x0,		0xb7,			3,		3},
		{DVBT_SYNC_LVL_PIP,					0x0,		0xb7,			2,		2},
		{DVBT_ERR_LVL_PIP,					0x0,		0xb7,			1,		1},
		{DVBT_VAL_LVL_PIP,					0x0,		0xb7,			0,		0},
		{DVBT_CKOUTPAR_PID,					0x0,		0xb9,			4,		4},
		{DVBT_CKOUT_PWR_PID,				0x0,		0xb9,			3,		3},
		{DVBT_SYNC_LVL_PID,					0x0,		0xb9,			2,		2},
		{DVBT_ERR_LVL_PID,					0x0,		0xb9,			1,		1},
		{DVBT_VAL_LVL_PID,					0x0,		0xb9,			0,		0},

		// FSM state-holding register
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_SM_PASS,						0x1,		0x93,			11,		0},

		// AD7 registers
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_AD7_SETTING,					0x0,		0x11,			15,		0},
		{DVBT_RSSI_R,						0x3,		0x1,			6,		0},

		// ACI detection registers
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_ACI_DET_IND,					0x3,		0x12,			0,		0},

		// Clock output registers
		// RegBitName,						PageNo,		RegStartAddr,	MSB,	LSB
		{DVBT_REG_MON,						0x0,		0xd,			1,		0},
		{DVBT_REG_MONSEL,					0x0,		0xd,			2,		2},
		{DVBT_REG_GPE,						0x0,		0xd,			7,		7},
		{DVBT_REG_GPO,						0x0,		0x10,			0,		0},
		{DVBT_REG_4MSEL,					0x0,		0x13,			0,		0},
"#;
