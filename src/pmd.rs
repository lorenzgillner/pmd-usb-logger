use serialport::SerialPort;
use std::thread;
use std::time::Duration;
use std::fmt::{Debug, Formatter};
use bincode::{deserialize, serialize, Options};
use serde::{Deserialize, Serialize};


const DEFAULT_BAUDRATE: u32 = 115200;
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
    WriteContTx,
    WriteConfigUart,
    ResetDevice = 0xF0,
    EnterBootloader = 0xF1,
    Nop = 0xFF,
}

#[repr(C, packed)]
#[derive(Deserialize, Debug, Default)]
pub struct DeviceIdStruct {
    pub vendor: u8,
    pub product: u8,
    pub firmware: u8,
}

#[repr(C, packed)]
#[derive(Deserialize, Default)]
pub struct ReadingStruct {
    pub name: [u8; PMD_SENSOR_NAME_LEN], // because in Rust, a `char` has 4 bytes!
    pub voltage: u16,
    pub current: u16,
    pub power: u16,
}

#[repr(C, packed)]
#[derive(Deserialize, Default)]
pub struct SensorStruct {
    pub sensor: [ReadingStruct; PMD_SENSOR_NUM],
}

#[repr(C, packed)]
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ConfigStruct {
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
    pub adc_gain_offset: [i8; PMD_ADC_CH_NUM],
    pub rsvd: [u8; 3],
}

#[repr(C, packed)]
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ContTxStruct {
    pub enable: u8,
    pub timestamp_bytes: u8,
    pub adc_channels: u8,
}

#[repr(C, packed)]
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct UartConfigStruct {
    pub baudrate: u32,
    pub parity: u32,
    pub data_width: u32,
    pub stop_bits: u32,
}

pub struct PMD {
    pub port: Box<dyn SerialPort>,
    device_id: DeviceIdStruct,
    config: ConfigStruct,
    sensors: SensorStruct,
}

impl PMD {
    pub fn new(port_name: &str) -> Self {
        let mut port = serialport::new(port_name, DEFAULT_BAUDRATE)
            .timeout(Duration::from_secs(5))
            .data_bits(serialport::DataBits::Eight)
            .stop_bits(serialport::StopBits::One)
            .parity(serialport::Parity::None)
            .open()
            .expect("Unable to open serial port");
        
        PMD {
            port,
            device_id: DeviceIdStruct::default(),
            config: ConfigStruct::default(),
            sensors: SensorStruct::default(),
        }
    }
    
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
    
    // XXX if this doesn't work, set AdcArray to [i16; 8]
    fn int16_from_adc(&self, value: u16) -> i16 {
        let value = value >> 4;
        if (value & 0x800) != 0 {
            value as i16 - 0x1000
        } else { 
            value as i16
        }
    }

    fn convert_voltage_adc_values(&self, value: u16, offset: i8) -> f64 {
        let value = self.int16_from_adc(value);
        (value + offset as i16) as f64 * PMD_ADC_VOLTAGE_SCALE
    }

    fn convert_current_adc_values(&self, value: u16, offset: i8) -> f64 {
        let value = self.int16_from_adc(value);
        (value + offset as i16) as f64 * PMD_ADC_CURRENT_SCALE
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

    pub fn convert_adc_values(&self, adc_values: &AdcBuffer) -> Vec<f64> {
        adc_values
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                if i % 2 == 0 {
                    self.convert_voltage_adc_values(v, self.config.adc_offset[i])
                } else {
                    self.convert_current_adc_values(v, self.config.adc_offset[i])
                }
            }).collect()
    }

    pub fn welcome(&mut self) {
        self.send_command(UartCommand::Welcome);
        let response = self.read_data(PMD_WELCOME_RESPONSE.len());
        assert_eq!(response, PMD_WELCOME_RESPONSE);
        log::debug!("> {}", std::str::from_utf8(&response).unwrap());
    }

    pub fn read_device_id(&mut self) -> DeviceIdStruct {
        self.send_command(UartCommand::ReadId);
        let rx_buffer = self.read_data(size_of::<DeviceIdStruct>());
        let device_id: DeviceIdStruct = deserialize(&rx_buffer).unwrap();
        assert_eq!(device_id.product, PMD_USB_PRODUCT_ID, "Invalid product ID");
        assert_eq!(device_id.vendor, PMD_USB_VENDOR_ID, "Invalid vendor ID");
        log::debug!("> Running firmware version {}", device_id.firmware);
        device_id
    }

    pub fn read_config(&mut self) -> ConfigStruct {
        self.send_command(UartCommand::ReadConfig);
        let rx_buffer = self.read_data(size_of::<ConfigStruct>());
        let config: ConfigStruct = deserialize(&rx_buffer).unwrap();
        config
    }

    pub fn write_config(&mut self) {
        todo!();
    }

    pub fn read_sensors(&mut self) -> SensorStruct {
        self.send_command(UartCommand::ReadSensors);
        let rx_buffer = self.read_data(size_of::<SensorStruct>());
        deserialize(&rx_buffer).unwrap()
    }

    pub fn read_sensor_values(&mut self) -> [u16; PMD_SENSOR_CH_NUM] {
        self.send_command(UartCommand::ReadSensorValues);
        let rx_buffer = self.read_data(PMD_SENSOR_BYTE_NUM);
        deserialize(&rx_buffer).unwrap()
    }

    pub fn read_adc_buffer(&mut self) -> AdcBuffer {
        self.send_command(UartCommand::ReadAdcBuffer);
        let rx_buffer = self.read_data(PMD_ADC_BYTE_NUM);
        deserialize(&rx_buffer).unwrap()
    }

    pub fn read_cont_tx(&mut self) -> AdcBuffer {
        let rx_buffer = self.read_data(PMD_ADC_BYTE_NUM);
        deserialize(&rx_buffer).unwrap()
    }

    fn clear_buffer(&mut self) {
        let mut scratch: Vec<u8> = Vec::new();
        let _ = self.port.read_to_end(&mut scratch);
        log::debug!("Cleared {} bytes", scratch.len());
    }

    pub fn write_config_cont_tx(&mut self, config: &ContTxStruct) {
        /* Tell the PMD to expect an incoming TX config */
        self.send_command(UartCommand::WriteContTx);

        /* Serialize the configuration struct back into a byte vector */
        let tx_buffer = serialize(config).unwrap();

        /* Send configuration */
        self.send_data(tx_buffer.as_slice());

        /* Wait for the device to apply new config */
        log::debug!("Waiting for device to process configuration");
        thread::sleep(Duration::from_millis(100));
    }

    pub fn enable_cont_tx(&mut self) {
        log::debug!("Starting cont TX");
        let config = ContTxStruct {
            enable: CONFIG_YES,
            timestamp_bytes: CONFIG_TIMESTAMP_NONE,
            adc_channels: CONFIG_MASK_ALL,
        };
        self.write_config_cont_tx(&config);
    }

    pub fn disable_cont_tx(&mut self) {
        log::debug!("Stopping cont TX");
        let config = ContTxStruct {
            enable: CONFIG_NO,
            timestamp_bytes: CONFIG_TIMESTAMP_NONE,
            adc_channels: CONFIG_MASK_NONE,
        };
        self.write_config_cont_tx(&config);
        self.clear_buffer();
    }

    pub fn init(&mut self) {
        self.disable_cont_tx();
        self.device_id = self.read_device_id();
        self.config = self.read_config();
        self.sensors = self.read_sensors();
        self.welcome();
    }
}

impl Debug for PMD {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.device_id, f)
    }
}
