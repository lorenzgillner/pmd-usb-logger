use bytemuck::{cast_slice, from_bytes, Pod, Zeroable};
use serialport::SerialPort;
use std::thread;
use std::time::Duration;
use std::convert::TryInto;
use std::fmt::{Debug, Formatter};
use bincode::{deserialize, serialize};
use serde::{Deserialize, Serialize};

pub const PMD_WELCOME_RESPONSE: &[u8; 17] = b"ElmorLabs PMD-USB";
pub const PMD_ADC_CH_NUM: usize = 8;
pub const PMD_ADC_BYTE_NUM: usize = size_of::<u16>() * PMD_ADC_CH_NUM;
pub const PMD_SENSOR_NUM: usize = 4;
pub const PMD_SENSOR_CH_NUM: usize = 2 * PMD_SENSOR_NUM;
pub const PMD_SENSOR_NAME_LEN: usize = 6;
pub const PMD_SENSOR_BYTE_NUM: usize = PMD_ADC_BYTE_NUM;
pub const PMD_USB_PRODUCT_ID: u8 = 0x0A;
pub const PMD_USB_VENDOR_ID: u8 = 0xEE;

const PMD_ADC_VOLTAGE_SCALE: f64 = 0.007568;
const PMD_ADC_CURRENT_SCALE: f64 = 0.0488;
const PMD_SENSOR_VOLTAGE_SCALE: f64 = 1.0 / 100.0;
const PMD_SENSOR_CURRENT_SCALE: f64 = 1.0 / 10.0;

pub const CONFIG_NO: u8 = 0x00;
pub const CONFIG_YES: u8 = 0x01;
pub const CONFIG_MASK_NONE: u8 = 0x00;
pub const CONFIG_MASK_ALL: u8 = 0xff;
pub const CONFIG_TIMESTAMP_NONE: u8 = 0x00;

pub type AdcBuffer = [u16; PMD_ADC_CH_NUM];

#[repr(u8)]
pub enum UartCommand {
    Welcome,
    ReadId,
    ReadSensors,
    ReadSensorValues,
    ReadConfig,
    WriteConfig,
    ReadAdcBuffer,
    WriteContinuousTx,
    WriteConfigUart,
    ResetDevice = 0xF0,
    EnterBootloader = 0xF1,
    Nop = 0xFF,
}

#[repr(u8)]
pub enum TimestampSize {
    None = 0x00,
    Small = 0x01,
    Medium = 0x02,
    Large = 0x04,
}

#[repr(C, packed)]
#[derive(Deserialize, Debug)]
pub struct DeviceIdStruct {
    pub vendor: u8,
    pub product: u8,
    pub firmware: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ReadingStruct {
    pub name: [u8; PMD_SENSOR_NAME_LEN], // because in Rust, a `char` has 4 bytes!
    pub voltage: u16,
    pub current: u16,
    pub power: u16,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct SensorStruct {
    pub sensor: [ReadingStruct; PMD_SENSOR_NUM],
}

#[repr(C, packed)]
#[derive(Copy, Clone, Pod, Zeroable)]
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

#[repr(C, packed)] // aligned
#[derive(Deserialize, Debug)]
pub struct ConfigStructV5 {
    pub version: u8,
    _pad1: u8,
    pub crc: u16,
    pub adc_offset: [i8; PMD_ADC_CH_NUM],
    pub oled_disable: u8,
    _pad2: u8,
    pub timeout_count: u16,
    pub timeout_action: u8,
    pub oled_speed: u8,
    pub restart_adc_flag: u8,
    pub cal_flag: u8,
    pub update_config_flag: u8,
    pub oled_rotation: u8,
    pub averaging: u8,
    // _pad3: u8,
    pub adc_gain_offset: [i8; PMD_ADC_CH_NUM],
    pub rsvd: [u8; 3],
}

#[repr(C, packed)]
// #[derive(Copy, Clone, Pod, Zeroable)]
#[derive(Serialize, Deserialize, Debug)]
pub struct ContTxStruct {
    pub enable: u8,
    pub timestamp_bytes: u8,
    pub adc_channels: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct UartConfigStruct {
    pub baudrate: u32,
    pub parity: u32,
    pub data_width: u32,
    pub stop_bits: u32,
}

pub struct PmdUsbDevice {
    port: Box<dyn SerialPort>,
    device_id: DeviceIdStruct,
    config: ContTxStruct,
    sensors: SensorStruct,
    calibration: [i8; PMD_ADC_CH_NUM],
    // tx_buffer: Vec<u8>,
    // rx_buffer: Vec<u8>,
}

impl PmdUsbDevice {
    fn send_command(&mut self, command: UartCommand) {
        let tx_buffer = command as u8;
        match self.port.write(&[tx_buffer]) {
            Ok(_) => log::debug!("Sending command {:#04X} to device", tx_buffer),
            Err(e) => panic!("Error while writing to device: {}", e),
        }
        self.port.flush().unwrap();
    }

    fn send_data(&mut self, data: &[u8]) {
        match self.port.write(data) {
            Ok(_) => log::debug!("Sending data to device: {:?}", data),
            Err(e) => panic!("Error while writing to device: {}", e),
        }
        self.port.flush().unwrap();
    }

    fn read_data(&mut self, expect: usize) -> Vec<u8> {
        let mut rx_buffer = vec![0u8; expect];
        match self.port.read_exact(&mut rx_buffer) {
            Ok(_) => rx_buffer,
            Err(e) => panic!("Error while reading from device: {}", e),
        }
    }

    fn convert_voltage_sensor_values(&self, value: u16) -> f64 {
        value as f64 * PMD_SENSOR_VOLTAGE_SCALE
    }

    fn convert_current_sensor_values(&self, value: u16) -> f64 {
        value as f64 * PMD_SENSOR_CURRENT_SCALE
    }

    fn convert_voltage_adc_values(&self, value: u16) -> f64 {
        value as f64 * PMD_ADC_VOLTAGE_SCALE
    }

    fn convert_current_adc_values(&self, value: u16) -> f64 {
        value as f64 * PMD_ADC_CURRENT_SCALE
    }

    pub fn welcome(&mut self) {
        self.send_command(UartCommand::Welcome);

        let response = self.read_data(PMD_WELCOME_RESPONSE.len());

        assert_eq!(response, PMD_WELCOME_RESPONSE);

        log::debug!("> {}", std::str::from_utf8(&response).unwrap());
    }

    pub fn read_id(&mut self) -> DeviceIdStruct {
        self.send_command(UartCommand::ReadId);

        let rx_buffer = self.read_data(size_of::<DeviceIdStruct>());

        let device_id: DeviceIdStruct = deserialize(&rx_buffer).unwrap();

        assert_eq!(device_id.product, PMD_USB_PRODUCT_ID, "Invalid product ID");
        assert_eq!(device_id.vendor, PMD_USB_VENDOR_ID, "Invalid vendor ID");

        log::debug!("> Running firmware version {}", device_id.firmware);

        device_id
    }

    pub fn read_sensors(&mut self) {
        self.send_command(UartCommand::ReadSensors);

        let buffer = self.read_data(size_of::<SensorStruct>());

        self.sensors = *parse_from_bytes::<SensorStruct>(&buffer);
    }

    pub fn read_sensor_values(&mut self) -> [u16; PMD_SENSOR_CH_NUM] {
        self.send_command(UartCommand::ReadSensorValues);

        let rx_buffer = self.read_data(PMD_SENSOR_BYTE_NUM);

        let sensor_values: [u16; PMD_SENSOR_CH_NUM] = cast_slice(&rx_buffer).try_into().unwrap();

        sensor_values
    }

    pub fn convert_sensor_values(&self, sensor_values: [u16; PMD_SENSOR_CH_NUM]) -> Vec<f64> {
        sensor_values
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                if i % 2 == 0 {
                    self.convert_voltage_sensor_values(v)
                } else {
                    self.convert_current_sensor_values(v)
                }
            }).collect()
    }

    pub fn read_continuous_tx(&mut self) -> AdcBuffer {
        /* Read a fixed number of bytes */
        let rx_buffer = self.read_data(PMD_ADC_BYTE_NUM);

        /* Convert received data to u16 */
        let adc_buffer: AdcBuffer = cast_slice(&rx_buffer).try_into().unwrap();

        /* Apply scaling and return */
        adc_buffer
    }

    pub fn convert_adc_values(&self, adc_values: &AdcBuffer) -> Vec<f64> {
        adc_values
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                if i % 2 == 0 {
                    self.convert_voltage_adc_values(v)
                } else {
                    self.convert_current_adc_values(v)
                }
            }).collect()
    }

    pub fn read_adc_buffer(&mut self) -> [u16; PMD_ADC_CH_NUM] {
        self.send_command(UartCommand::ReadAdcBuffer);

        let rx_buffer = self.read_data(PMD_ADC_BYTE_NUM);

        cast_slice(&rx_buffer).try_into().unwrap()
    }

    fn clear_buffer(&mut self) {
        let mut scratch: Vec<u8> = Vec::new();
        let _ = self.port.read_to_end(&mut scratch);
        log::debug!("Cleared {} bytes: {:?}", scratch.len(), scratch);
    }

    pub fn write_config_continuous_tx(&mut self, config: &ContTxStruct) {
        /* Tell the PMD to expect an incoming TX config */
        self.send_command(UartCommand::WriteContinuousTx);

        /* Serialize the configuration struct back into a byte vector */
        let tx_buffer = serialize(config).unwrap();

        /* Send configuration */
        self.send_data(tx_buffer.as_slice());

        /* Wait for the device to apply new config */
        log::debug!("Waiting for device to process configuration");
        thread::sleep(Duration::from_millis(100));
    }

    pub fn enable_continuous_tx(&mut self) {
        log::debug!("Starting continuous TX");
        let config = ContTxStruct {
            enable: CONFIG_YES,
            timestamp_bytes: CONFIG_TIMESTAMP_NONE,
            adc_channels: CONFIG_MASK_ALL,
        };
        self.write_config_continuous_tx(&config);
    }

    pub fn disable_continuous_tx(&mut self) {
        log::debug!("Stopping continuous TX");
        let config = ContTxStruct {
            enable: CONFIG_NO,
            timestamp_bytes: CONFIG_TIMESTAMP_NONE,
            adc_channels: CONFIG_MASK_NONE,
        };
        self.write_config_continuous_tx(&config);
        self.clear_buffer();
    }

    pub fn read_config(&mut self) {
        todo!();
    }

    pub fn write_config(&mut self) {
        todo!();
    }

    pub fn read_calibration(&mut self) {
        self.send_command(UartCommand::ReadConfig);

        if self.device_id.firmware < 6 {
            let config = self.read_data(size_of::<ConfigStruct>());
            let config: ConfigStruct = *parse_from_bytes::<ConfigStruct>(&config);
            self.calibration[..PMD_ADC_CH_NUM].copy_from_slice(&config.adc_offset[..PMD_ADC_CH_NUM]); // TODO this is redundant
        } else {
            let config = self.read_data(size_of::<ConfigStructV5>());
            let config: ConfigStructV5 = deserialize(&config).unwrap();
            self.calibration[..PMD_ADC_CH_NUM].copy_from_slice(&config.adc_offset[..PMD_ADC_CH_NUM]); // TODO this is redundant
            log::debug!("Config: {:?}", config.adc_offset);
        }
    }
}

impl Debug for PmdUsbDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.device_id, f)
    }
}

// TODO replace this with serde
pub fn parse_from_bytes<T: Pod>(buffer: &[u8]) -> &T {
    assert_eq!(buffer.len(), size_of::<T>(), "Buffer size mismatch");
    from_bytes(buffer)
}
