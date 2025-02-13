use serialport::SerialPort;
use std::intrinsics::transmute;
use std::io::{Read, Write};
use std::time::Duration;
use std::{io, thread};
//use log::{debug};

use crate::pmd;

pub type SensorArray = [u16; PMD_SENSOR_CH_NUM];
pub type AdcArray = [u16; PMD_ADC_CH_NUM];

pub const CMD_WELCOME_MESSAGE: u8 = 0x00;
pub const CMD_READ_ID: u8 = 0x01;
pub const CMD_READ_SENSORS: u8 = 0x02;
pub const CMD_READ_SENSOR_VALUES: u8 = 0x03;
pub const CMD_READ_CONFIG: u8 = 0x04;
pub const CMD_WRITE_CONFIG: u8 = 0x05;
pub const CMD_READ_ADC_BUFFER: u8 = 0x06;
pub const CMD_WRITE_CONTINUOUS_TX: u8 = 0x07;
pub const CMD_WRITE_CONFIG_UART: u8 = 0x08;
pub const CMD_RESET_DEVICE: u8 = 0xF0;
pub const CMD_ENTER_BOOTLOADER: u8 = 0xF1;
pub const CMD_NOP: u8 = 0xFF;

pub const PMD_WELCOME_RESPONSE: &[u8; 17] = b"ElmorLabs PMD-USB";
pub const PMD_ADC_CH_NUM: usize = 8;
pub const PMD_ADC_BYTE_NUM: usize = size_of::<u16>() * PMD_ADC_CH_NUM;
pub const PMD_SENSOR_NUM: usize = 4;
pub const PMD_SENSOR_CH_NUM: usize = 2 * PMD_SENSOR_NUM;
pub const PMD_SENSOR_NAME_LEN: usize = 6;
pub const PMD_SENSOR_BYTE_NUM: usize = PMD_ADC_BYTE_NUM;

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

fn send_command(port: &mut dyn SerialPort, command: u8) {
    match port.write(&[command]) {
        Ok(_) => println!("Sending command {:#X} to device", command), // TODO debug!
        Err(e) => panic!("Error while writing to device: {}", e),
    }

    port.flush().unwrap();
}

fn read_response(port: &mut dyn SerialPort, bytes: usize) -> Vec<u8> {
    let mut buffer = [0u8; bytes]; // TODO this
    match port.read_exact(&mut buffer) {
        Ok(_) => {
            println!("Received response from device");
            return buffer.to_vec();
        }
        Err(e) => panic!("Error while reading from device: {}", e),
    }
}

pub fn welcome(port: &mut dyn SerialPort) {
    send_command(port, CMD_WELCOME_MESSAGE);

    let buffer = read_response(port, PMD_WELCOME_RESPONSE.len());

    assert_eq!(buffer.as_slice(), PMD_WELCOME_RESPONSE);
    
    println!("> {}", std::str::from_utf8(&buffer).unwrap()); // TODO debug!
}

pub fn read_id(port: &mut dyn SerialPort) -> DeviceIdStruct {
    send_command(port, CMD_READ_ID);

    let mut device_id_buffer = [0u8; size_of::<DeviceIdStruct>()];

    match port.read_exact(&mut device_id_buffer) {
        Ok(_) => println!("Received {} bytes", size_of::<DeviceIdStruct>()), // TODO debug!
        Err(e) => panic!("Error while reading from device: {}", e),
    }

    let device_id: DeviceIdStruct = unsafe { transmute(device_id_buffer) };

    println!("> Running firmware version {}", device_id.firmware); // TODO debug!

    return device_id;
}

pub fn read_sensors(port: &mut dyn SerialPort) {
    send_command(port, CMD_READ_SENSORS);

    let mut sensor_buffer = [0u8; size_of::<SensorStruct>()];

    match port.read_exact(&mut sensor_buffer) {
        Ok(_) => println!("Received {} bytes of data", size_of::<SensorStruct>()), // TODO debug!
        Err(e) => panic!("Error while reading from device: {}", e),
    }

    let sensors: SensorStruct = unsafe { transmute(sensor_buffer) };

    for sen in sensors.sensor {
        println!(
            "> Sensor {}: Voltage={}, Current={}, Power={}",
            std::str::from_utf8(&sen.name).unwrap().trim(),
            sen.voltage,
            sen.current,
            sen.power
        ); // TODO debug!
    }
}

pub fn read_sensor_values(port: &mut dyn SerialPort) -> SensorArray {
    send_command(port, CMD_READ_SENSOR_VALUES);

    let mut sensor_values = [0u8; PMD_SENSOR_BYTE_NUM];

    match port.read_exact(&mut sensor_values) {
        Ok(_) => println!("Received {} bytes of sensor data", PMD_SENSOR_BYTE_NUM), // TODO debug!
        Err(e) => panic!("Error while reading from device: {}", e),
    }

    let sensor_values: [u16; PMD_SENSOR_CH_NUM] = unsafe { transmute(sensor_values) };

    /* PCIE1 voltage (1e-2 V), PCIE1 current (1e-1 A), PCIE2 ... */
    return sensor_values;
}

pub fn read_adc_buffer(port: &mut dyn SerialPort) -> AdcArray {
    send_command(port, CMD_READ_ADC_BUFFER);

    let mut adc_buffer = [0u8; PMD_ADC_BYTE_NUM];

    match port.read_exact(&mut adc_buffer) {
        Ok(_) => println!("Received {} bytes of sensor data", PMD_ADC_BYTE_NUM), // TODO debug!
        Err(e) => panic!("Error while reading from device: {}", e),
    }

    let adc_buffer: [u16; PMD_ADC_CH_NUM] = unsafe { transmute(adc_buffer) };

    return adc_buffer;
}

pub fn write_config_cont_tx(port: &mut dyn SerialPort, config: &ContTxStruct) {
    send_command(port, CMD_WRITE_CONTINUOUS_TX);
    
    match port.write(&[config.enable, config.timestamp_bytes, config.adc_channels]) {
        Ok(_) => println!("Configuring device"), // TODO debug!
        Err(e) => panic!("Error while writing to device: {}", e),
    }

    port.flush().unwrap();

    println!("Waiting for device to process configuration"); // TODO debug!
    thread::sleep(Duration::from_secs(1));

    let mut scratch = [0u8; PMD_WELCOME_RESPONSE.len()];
    match port.read_exact(&mut scratch) {
        Ok(_) => println!("Device is ready"), // TODO debug!
        Err(e) => panic!("Error while reading from device: {}", e),
    }

    println!("> {}", std::str::from_utf8(&scratch).unwrap()); // TODO debug!
}

pub fn prime_connection(port: &mut dyn SerialPort) {
    println!("Stopping continuous TX"); // TODO debug!
    let config = ContTxStruct {
        enable: CONFIG_FALSE,
        timestamp_bytes: CONFIG_TIMESTAMP_NONE,
        adc_channels: CONFIG_MASK_NONE,
    };
    write_config_cont_tx(&mut *port, &config);
}

// TODO read_config(); tricky because of different config versions

// TODO write_config()

pub fn read_calibration(port: &mut dyn SerialPort, calibration: &mut [[i8; 2]; PMD_ADC_CH_NUM]) {
    let device_id: DeviceIdStruct = read_id(&mut *port);

    match port.write(&[CMD_READ_CONFIG]) {
        Ok(_) => println!("Reading device config"), // TODO debug!
        Err(e) => panic!("Error while writing to device: {}", e),
    }

    port.flush().unwrap();

    if device_id.firmware < 6 {
        let mut config = [0u8; size_of::<ConfigStruct>()];
        match port.read_exact(&mut config) {
            Ok(_) => println!("Received config data"), // TODO debug!
            Err(e) => panic!("Error while reading from device: {}", e),
        }
        let config: ConfigStruct = unsafe { transmute(config) };
        for i in 0..PMD_ADC_CH_NUM {
            calibration[i][0] = config.adc_offset[i];
        }
    } else {
        let mut config = [0u8; size_of::<ConfigStructV5>()];
        match port.read_exact(&mut config) {
            Ok(_) => println!("Received config data V5"), // TODO debug!
            Err(e) => panic!("Error while reading from device: {}", e),
        }
        let config: ConfigStructV5 = unsafe { transmute(config) };
        for i in 0..PMD_ADC_CH_NUM {
            calibration[i][0] = config.adc_offset[i];
            calibration[i][1] = config.adc_gain_offset[i];
        }
    }

    println!("Calibration data: {:?}", calibration); // TODO debug!
}