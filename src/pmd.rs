use bincode::{deserialize, serialize};
use serde::{Deserialize, Serialize};
use serialport::SerialPort;
use std::fmt::Debug;
use std::io::Write;
use std::thread;
use std::time::Duration;

const BAUDRATE_DEFAULT: u32 = 115_200;
const BAUDRATE_FASTEST: u32 = 460_800; //345_600;//230_400;

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
const PMD_CLOCK_MULTIPLIER: f64 = 1.0 / 3.0;
const PMD_TIMEOUT_SECS: u64 = 1;

pub const CONFIG_NO: u8 = 0x00;
pub const CONFIG_YES: u8 = 0x01;
pub const CONFIG_MASK_NONE: u8 = 0x00;
pub const CONFIG_MASK_ALL: u8 = 0xff;
pub const CONFIG_TIMESTAMP_NONE: u8 = 0x00;
// pub const CONFIG_TIMESTAMP_LOW: u8 = 0x01;
// pub const CONFIG_TIMESTAMP_MED: u8 = 0x02;
pub const CONFIG_TIMESTAMP_FULL: u8 = 0x04;
pub const CONFIG_UART_PARITY_NONE: u32 = 0x2;
pub const CONFIG_UART_DATA_WIDTH_EIGHT: u32 = 0x0;
pub const CONFIG_UART_STOP_BITS_ONE: u32 = 0x0;

pub type SensorBuffer = [u16; PMD_SENSOR_CH_NUM];
pub type AdcBuffer = [u16; PMD_ADC_CH_NUM];
pub type SensorValues = [f64; PMD_SENSOR_CH_NUM];

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
pub struct TimedAdcBuffer {
    pub timestamp: u32,
    pub buffer: AdcBuffer,
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

impl Debug for ReadingStruct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "\"{}\": {{\n\tvoltage: {:.02}V\n\tcurrent: {:.02}A\n\tpower: {}W\n}}",
            std::str::from_utf8(&self.name).unwrap().trim(),
            self.voltage as f64 * PMD_SENSOR_VOLTAGE_SCALE,
            self.current as f64 * PMD_SENSOR_CURRENT_SCALE,
            self.power as f64
        )
    }
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
    _pad1: u8, // relevant because of mixed integers
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
    pub baud_rate: u32,
    pub parity: u32,
    pub data_width: u32,
    pub stop_bits: u32,
}

pub struct PmdUsb {
    port: Box<dyn SerialPort>,
    device_id: DeviceIdStruct,
    config: ConfigStruct,
    sensors: SensorStruct,
}

impl PmdUsb {
    pub fn new(port_name: &str) -> Self {
        let port = serialport::new(port_name, BAUDRATE_DEFAULT)
            .timeout(Duration::from_millis(100))
            .data_bits(serialport::DataBits::Eight)
            .stop_bits(serialport::StopBits::One)
            .parity(serialport::Parity::None)
            // .flow_control(serialport::FlowControl::Hardware)
            .open()
            .expect("Unable to open serial port");

        PmdUsb {
            port,
            device_id: DeviceIdStruct::default(),
            config: ConfigStruct::default(),
            sensors: SensorStruct::default(),
        }
    }

    fn send_command(&mut self, command: UartCommand) {
        self.clear_buffers();
        let tx_buffer = command as u8;
        match self.port.write(&[tx_buffer]) {
            Ok(_) => log::debug!("Sending command {:#04X} to device", tx_buffer),
            Err(e) => panic!("Error while writing to device: {}", e),
        }
        self.port.flush().unwrap();
    }

    fn send_data(&mut self, data: &[u8]) {
        self.clear_buffers();
        match self.port.write_all(data) {
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

    pub fn convert_sensor_values(&self, sensor_values: &SensorBuffer) -> SensorValues {
        let mut _sensor_values: SensorValues = Default::default();
        for i in (0..sensor_values.len()).step_by(2) {
            let j = i + 1;
            _sensor_values[i] = self.convert_voltage_sensor_values(sensor_values[i]);
            _sensor_values[j] = self.convert_current_sensor_values(sensor_values[j])
        }
        _sensor_values
    }

    fn convert_voltage_adc_values(&self, value: u16, offset: i8) -> f64 {
        let value = i16_from_adc(value);
        (value + (offset as i16)) as f64 * PMD_ADC_VOLTAGE_SCALE
    }

    fn convert_current_adc_values(&self, value: u16, offset: i8) -> f64 {
        let value = i16_from_adc(value);
        (value + (offset as i16)) as f64 * PMD_ADC_CURRENT_SCALE
    }

    pub fn convert_adc_values(&self, adc_values: &AdcBuffer) -> SensorValues {
        let mut _adc_values: SensorValues = Default::default();
        for i in (0..adc_values.len()).step_by(2) {
            let j = i + 1;
            _adc_values[i] =
                self.convert_voltage_adc_values(adc_values[i], self.config.adc_offset[i]);
            _adc_values[j] =
                self.convert_current_adc_values(adc_values[j], self.config.adc_offset[j]);
        }
        _adc_values
    }

    pub fn welcome(&mut self) {
        self.send_command(UartCommand::Welcome);
        let response = self.read_data(PMD_WELCOME_RESPONSE.len());
        assert_eq!(response, PMD_WELCOME_RESPONSE);
        log::debug!("> {}", std::str::from_utf8(&response).unwrap());
    }

    pub fn read_device_id(&mut self) -> DeviceIdStruct {
        self.clear_buffers();
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

    pub fn read_sensor_values(&mut self) -> SensorBuffer {
        self.send_command(UartCommand::ReadSensorValues);
        let rx_buffer = self.read_data(PMD_SENSOR_BYTE_NUM);
        deserialize(&rx_buffer).unwrap()
    }

    pub fn read_adc_buffer(&mut self) -> AdcBuffer {
        self.send_command(UartCommand::ReadAdcBuffer);
        let rx_buffer = self.read_data(PMD_ADC_BYTE_NUM);
        deserialize(&rx_buffer).unwrap()
    }

    pub fn read_cont_tx(&mut self) -> TimedAdcBuffer {
        let n_bytes = size_of::<TimedAdcBuffer>();
        let rx_buffer = self.read_data(n_bytes);
        deserialize(&rx_buffer).unwrap()
    }

    fn clear_buffers(&mut self) {
        match self.port.clear(serialport::ClearBuffer::All) {
            Ok(_) => (),
            Err(e) => panic!("Error while clearing serial port: {}", e),
        };
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
        self.clear_buffers();
        log::debug!("Starting cont TX");
        let config = ContTxStruct {
            enable: CONFIG_YES,
            timestamp_bytes: CONFIG_TIMESTAMP_FULL,
            adc_channels: CONFIG_MASK_ALL,
        };
        self.write_config_cont_tx(&config);
    }

    pub fn disable_cont_tx(&mut self) {
        log::debug!("Stopping cont TX");
        let config = ContTxStruct {
            enable: CONFIG_NO,
            timestamp_bytes: CONFIG_TIMESTAMP_FULL, //CONFIG_TIMESTAMP_NONE,
            adc_channels: CONFIG_MASK_ALL,          //CONFIG_MASK_NONE,
        };
        self.write_config_cont_tx(&config);
        self.clear_buffers();
    }

    fn set_baud_rate(&mut self, baud_rate: u32) {
        log::debug!("Setting baud rate to {}", baud_rate);
        self.send_command(UartCommand::WriteConfigUart);
        let config = UartConfigStruct {
            baud_rate,
            parity: CONFIG_UART_PARITY_NONE,
            data_width: CONFIG_UART_DATA_WIDTH_EIGHT,
            stop_bits: CONFIG_UART_STOP_BITS_ONE,
        };
        let tx_buffer = serialize(&config).unwrap();
        self.send_data(tx_buffer.as_slice());
        thread::sleep(Duration::from_secs(PMD_TIMEOUT_SECS));
        match self.port.set_baud_rate(baud_rate) {
            Ok(_) => {}
            Err(e) => panic!("Failed to set baud rate: {}", e),
        }
        thread::sleep(Duration::from_millis(500));
        self.clear_buffers();
    }

    pub fn bump_baud_rate(&mut self) {
        self.set_baud_rate(BAUDRATE_FASTEST);
    }

    pub fn restore_baud_rate(&mut self) {
        self.set_baud_rate(BAUDRATE_DEFAULT);
    }

    pub fn init(&mut self) {
        self.disable_cont_tx();
        self.clear_buffers();
        self.device_id = self.read_device_id();
        self.config = self.read_config();
        self.sensors = self.read_sensors();
        self.welcome();
    }
}

/// Scale the device-side timestamp (approx. 3 MHz) to microseconds
pub fn adjust_device_timestamp(timestamp: u32) -> u128 {
    let _timestamp = timestamp as f64;
    (_timestamp * PMD_CLOCK_MULTIPLIER).floor() as u128
}

/// Little helper to convert signed 12-bit integers from the ADC to i16
fn i16_from_adc(value: u16) -> i16 {
    let value = value >> 4;

    let value = value & 0x0FFF;

    if (value & 0x0800) != 0 {
        (value | 0xF000) as i16
    } else {
        value as i16
    }
}
