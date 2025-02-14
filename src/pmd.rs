use bytemuck::{cast_slice, from_bytes, Pod, Zeroable};
use serialport::SerialPort;
use std::thread;
use std::time::Duration;

pub type SensorArray = [u16; PMD_SENSOR_CH_NUM];
pub type AdcArray = [u16; PMD_ADC_CH_NUM];

pub const PMD_WELCOME_RESPONSE: &[u8; 17] = b"ElmorLabs PMD-USB";
pub const PMD_ADC_CH_NUM: usize = 8;
pub const PMD_ADC_BYTE_NUM: usize = size_of::<u16>() * PMD_ADC_CH_NUM;
pub const PMD_SENSOR_NUM: usize = 4;
pub const PMD_SENSOR_CH_NUM: usize = 2 * PMD_SENSOR_NUM;
pub const PMD_SENSOR_NAME_LEN: usize = 6;
pub const PMD_SENSOR_BYTE_NUM: usize = PMD_ADC_BYTE_NUM;
pub const PMD_PRODUCT_ID: u8 = 0x0A;
pub const PMD_VENDOR_ID: u8 = 0xEE;

pub const CONFIG_NO: u8 = 0x00;
pub const CONFIG_YES: u8 = 0x01;
pub const CONFIG_MASK_NONE: u8 = 0x00;
pub const CONFIG_MASK_ALL: u8 = 0xff;
pub const CONFIG_TIMESTAMP_NONE: u8 = 0x00;
pub const CONFIG_TIMESTAMP_FULL: u8 = 0x04;

#[repr(u8)]
pub enum Command {
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
#[derive(Copy, Clone, Pod, Zeroable)]
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

#[repr(C, packed)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
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

#[repr(C, packed)]
#[derive(Copy, Clone, Pod, Zeroable)]
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

pub fn parse_from_bytes<T: Pod>(buffer: &[u8]) -> &T {
    assert_eq!(buffer.len(), size_of::<T>(), "Buffer size mismatch");
    from_bytes(buffer)
}

fn send_command(port: &mut dyn SerialPort, command: Command) {
    let command = command as u8;
    match port.write(&[command]) {
        Ok(_) => log::debug!("Sending command {:#04X} to device", command),
        Err(e) => panic!("Error while writing to device: {}", e),
    }
    port.flush().unwrap();
}

fn send_data(port: &mut dyn SerialPort, data: &[u8]) {
    match port.write(data) {
        Ok(_) => log::debug!("Sending data to device: {:?}", data),
        Err(e) => panic!("Error while writing to device: {}", e),
    }
    port.flush().unwrap();
}

fn read_response(port: &mut dyn SerialPort, expect: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; expect];
    match port.read_exact(&mut buffer) {
        Ok(_) => buffer,
        Err(e) => panic!("Error while reading from device: {}", e),
    }
}

pub fn welcome(port: &mut dyn SerialPort) {
    send_command(port, Command::Welcome);

    let response = read_response(port, PMD_WELCOME_RESPONSE.len());

    assert_eq!(response, PMD_WELCOME_RESPONSE);

    log::debug!("> {}", std::str::from_utf8(&response).unwrap());
}

pub fn read_id(port: &mut dyn SerialPort) -> DeviceIdStruct {
    send_command(port, Command::ReadId);

    let buffer = read_response(port, size_of::<DeviceIdStruct>());
    let device_id = *parse_from_bytes::<DeviceIdStruct>(&buffer);

    assert_eq!(device_id.product, PMD_PRODUCT_ID, "Invalid product ID");
    assert_eq!(device_id.vendor, PMD_VENDOR_ID, "Invalid vendor ID");

    log::debug!("> Running firmware version {}", device_id.firmware);

    device_id
}

pub fn read_sensors(port: &mut dyn SerialPort) {
    send_command(port, Command::ReadSensors);

    let buffer = read_response(port, size_of::<SensorStruct>());
    let sensors = *parse_from_bytes::<SensorStruct>(&buffer);

    sensors.sensor.iter().for_each(|sen| {
        let name = std::str::from_utf8(&sen.name).unwrap().trim();
        let voltage = sen.voltage;
        let current = sen.current;
        let power = sen.power;

        log::debug!(
            "> Sensor {}: Voltage={}, Current={}, Power={}",
            name,
            voltage,
            current,
            power
        );
    });
}

pub fn read_sensor_values(port: &mut dyn SerialPort) -> SensorArray {
    send_command(port, Command::ReadSensorValues);

    let buffer = read_response(port, PMD_SENSOR_BYTE_NUM);

    let sensor_values: [u16; PMD_SENSOR_CH_NUM] = cast_slice(&buffer).try_into().unwrap();

    sensor_values /* PCIE1 voltage (1e-2 V), PCIE1 current (1e-1 A), PCIE2 ... */
}

pub fn read_adc_buffer(port: &mut dyn SerialPort) -> AdcArray {
    send_command(port, Command::ReadAdcBuffer);

    let buffer = read_response(port, PMD_ADC_BYTE_NUM);

    let adc_buffer: [u16; PMD_ADC_CH_NUM] = cast_slice(&buffer).try_into().unwrap();

    adc_buffer /* same as above */
}

pub fn write_config_cont_tx(port: &mut dyn SerialPort, config: &ContTxStruct) {
    /* Tell the PMD to expect an incoming config */
    send_command(port, Command::WriteContinuousTx);

    /* Send configuration bytes */
    send_data(port, &[config.enable, config.timestamp_bytes, config.adc_channels]);

    log::debug!("Waiting for device to process configuration");

    /* Wait for the device to apply new config */
    thread::sleep(Duration::from_millis(1000));
}

pub fn disable_continuous_tx(port: &mut dyn SerialPort) {
    log::debug!("Stopping continuous TX");
    let config = ContTxStruct {
        enable: CONFIG_NO,
        timestamp_bytes: CONFIG_TIMESTAMP_NONE,
        adc_channels: CONFIG_MASK_NONE,
    };
    write_config_cont_tx(&mut *port, &config);

    let mut scratch: Vec<u8> = Vec::new();
    port.read_to_end(&mut scratch);
}

pub fn enable_continuous_tx(port: &mut dyn SerialPort, timestamp: TimestampSize) {
    log::debug!("Starting continuous TX");
    let config = ContTxStruct {
        enable: CONFIG_YES,
        timestamp_bytes: timestamp as u8,
        adc_channels: CONFIG_MASK_ALL,
    };
    write_config_cont_tx(&mut *port, &config);
}

// TODO read_config(); tricky because of different config versions

// TODO write_config()

pub fn read_calibration(port: &mut dyn SerialPort, calibration: &mut [[i8; 2]; PMD_ADC_CH_NUM]) {
    let device_id: DeviceIdStruct = read_id(&mut *port);

    send_command(port, Command::ReadConfig);

    if device_id.firmware < 6 {
        let mut config = [0u8; size_of::<ConfigStruct>()];
        match port.read_exact(&mut config) {
            Ok(_) => log::debug!("Received config data"),
            Err(e) => panic!("Error while reading from device: {}", e),
        }
        let config: ConfigStruct = *parse_from_bytes::<ConfigStruct>(&config);
        for i in 0..PMD_ADC_CH_NUM {
            calibration[i][0] = config.adc_offset[i];
        }
    } else {
        let mut config = [0u8; size_of::<ConfigStructV5>()];
        match port.read_exact(&mut config) {
            Ok(_) => log::debug!("Received config data V5"),
            Err(e) => panic!("Error while reading from device: {}", e),
        }
        let config: ConfigStructV5 = *parse_from_bytes::<ConfigStructV5>(&config);
        for i in 0..PMD_ADC_CH_NUM {
            calibration[i][0] = config.adc_offset[i];
            calibration[i][1] = config.adc_gain_offset[i];
        }
        log::debug!("Config: {:?}", config);
    }
}
