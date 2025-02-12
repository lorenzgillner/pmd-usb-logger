pub const CMD_WELCOME_MESSAGE: u8 = 0x00;
pub const CMD_READ_ID: u8 = 0x01;
pub const CMD_READ_SENSORS: u8 = 0x02;
pub const CMD_READ_VALUES: u8 = 0x03;
pub const CMD_READ_CONFIG: u8 = 0x04;
pub const CMD_READ_ADC_BUFFER: u8 = 0x06;
pub const CMD_WRITE_CONTINUOUS_TX: u8 = 0x07;
pub const CMD_WRITE_CONFIG_UART: u8 = 0x08;
pub const CMD_RESET_DEVICE: u8 = 0xF0;
pub const CMD_ENTER_BOOTLOADER: u8 = 0xF1;
pub const CMD_NOP: u8 = 0xFF;

pub const PMD_RESPONSE: &[u8; 17] = b"ElmorLabs PMD-USB";
pub const PMD_SENSOR_NUM: usize = 4;
pub const PMD_SENSOR_NAME_LEN: usize = 6;
pub const PMD_SENSOR_BYTE_NUM: usize = 16;
pub const PMD_ADC_CH_NUM: usize = 8;
pub const PMD_ADC_BYTE_NUM: usize = 16;

pub const CONFIG_ENABLE: u8 = 0x01;
pub const CONFIG_TRUE: u8 = 0x01;
pub const CONFIG_DISABLE: u8 = 0x00;
pub const CONFIG_FALSE: u8 = 0x00;
pub const CONFIG_MASK_NONE: u8 = 0x00;
pub const CONFIG_MASK_ALL: u8 = 0xff;
pub const CONFIG_TIMESTAMP_NONE: u8 = 0x00;
pub const CONFIG_TIMESTAMP_FULL: u8 = 0x04;

#[repr(C)]
pub struct DeviceIdStruct {
    pub vendor: u8,
    pub product: u8,
    pub firmware: u8,
}

#[repr(C)]
pub struct ReadingStruct {
    pub name: [u8; PMD_SENSOR_NAME_LEN], // because in Rust, a `char` has 4 bytes!
    pub voltage: u16,
    pub current: u16,
    pub power: u16,
}

#[repr(C)]
pub struct SensorStruct {
    pub sensor: [ReadingStruct; PMD_SENSOR_NUM],
}

#[repr(C)]
pub struct ConfigStruct {
    pub version: u8,
    pub crc: u16,
    pub adc_offset: [i8; PMD_ADC_CH_NUM],
    pub oled_disable: u8,
    pub timeout_count: u16,
    pub timeout_action: u8,
    pub oled_speed: u8,
    pub restart_adc_flag: u8,
    pub cal_flag: u8,
    pub update_config_flag: u8,
    pub oled_rotation: u8,
    pub averaging: u8,
    pub rsvd: [u8; 3],
}

#[repr(C)]
pub struct ConfigStructV5 {
    pub version: u8,
    pub crc: u16,
    pub adc_offset: [i8; PMD_ADC_CH_NUM],
    pub oled_disable: u8,
    pub timeout_count: u16,
    pub timeout_action: u8,
    pub oled_speed: u8,
    pub restart_adc_flag: u8,
    pub cal_flag: u8,
    pub update_config_flag: u8,
    pub oled_rotation: u8,
    pub averaging: u8,
    pub adc_gain_offset: [i8; PMD_ADC_CH_NUM],
    pub rsvd: [u8; 3],
}

#[repr(C)]
pub struct ContTxStruct {
    pub enable: u8,
    pub timestamp_bytes: u8,
    pub adc_channels: u8,
}

#[repr(C)]
pub struct UartConfigStruct {
    pub baudrate: u32,
    pub parity: u32,
    pub data_width: u32,
    pub stop_bits: u32,
}