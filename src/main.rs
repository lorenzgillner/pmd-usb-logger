mod pmd;

use std::{env, io};
use std::thread;
use std::io::{Read, Write};
use std::mem::transmute;
use chrono::Local;
use std::time::Duration;
use clap::{Arg, Command};
use env_logger;
use serialport;
use serialport::SerialPort;
use crate::pmd::{SensorStruct, DeviceIdStruct, ContTxStruct, ConfigStruct, ConfigStructV5};

fn main() {
    /* Set up command line options */
    let args = Command::new("pmd-usb-logger")
        .arg(Arg::new("port")
            .short('p')
            .long("port")
            .value_name("PORT")
            .help("Serial port to use, e.g. /dev/ttyUSB0 or COM0")
            .default_value("/dev/ttyUSB0"))
        .arg(Arg::new("baudrate")
            .short('b')
            .long("baudrate")
            .value_name("BAUDRATE")
            .help("Baud rate to use, e.g. 115200")
            .default_value("115200"))
        .arg(Arg::new("verbose")
            .short('v')
            .long("verbose")
            .help("Enable verbose output")
            .action(clap::ArgAction::Count))
    .get_matches();

    /* Dispatch command line options */
    let port_name = args.get_one::<String>("port").unwrap();
    let baudrate: u32 = args.get_one::<String>("baudrate").unwrap().parse().expect("Invalid baud rate");
    let log_level = match args.get_count("verbose") {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };

    /* Configure verbosity level */
    env::set_var("RUST_LOG", log_level);
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(buf, "{} [{}]: {}", Local::now().to_string(), record.level(), record.args())
        })
        .init();

    /* Give the user some feedback */
    println!("Selected device {} at baud rate {}", port_name, baudrate);

    /* Initialize serial connection */
    let mut port = match serialport::new(port_name, baudrate)
        .timeout(Duration::from_secs(5))
        .data_bits(serialport::DataBits::Eight)
        .stop_bits(serialport::StopBits::One)
        .open() {
        Ok(p) => p,
        Err(e) => panic!("Unable to open serial port: {}", e),
    };

    let mut calibration: [i8; pmd::PMD_ADC_CH_NUM] = [0; pmd::PMD_ADC_CH_NUM];

    test_connection(&mut *port);
    prime_connection(&mut *port);
    read_calibration(&mut *port, &mut calibration);
}

fn test_connection(port: &mut dyn SerialPort) {
    /* Sanity check */
    match port.write(&[pmd::CMD_WELCOME_MESSAGE]) {
        Ok(_) => println!("Successfully wrote to device"),
        Err(ref e) if e.kind() == io::ErrorKind::TimedOut => println!("Timed out while writing to device"),
        Err(e) => panic!("Error while writing to device: {}", e),
    }

    port.flush().unwrap();

    let mut welcome_buffer = [0u8; 17];

    match port.read_exact(&mut welcome_buffer) {
        Ok(_) => {
            assert_eq!(&welcome_buffer, pmd::PMD_RESPONSE);
            println!("Received response from device: {}", std::str::from_utf8(&welcome_buffer).unwrap());
        },
        Err(e) => panic!("Error while reading from device: {}", e),
    }

    /* Sensor check */
    match port.write(&[pmd::CMD_READ_SENSORS]) {
        Ok(_) => println!("Successfully wrote to device"),
        Err(e) => panic!("Error while writing to device: {}", e),
    }

    port.flush().unwrap();

    let mut sensor_buffer = [0u8; size_of::<SensorStruct>()];

    println!("Expecting {} bytes of data", size_of::<SensorStruct>());

    match port.read_exact(&mut sensor_buffer) {
        Ok(_) => println!("Received {} bytes", size_of::<SensorStruct>()),
        Err(e) => panic!("Error while reading from device: {}", e),
    }

    let sensors: SensorStruct = unsafe {
        transmute(sensor_buffer)
    };

    for sen in sensors.sensor {
        println!("> Hello from sensor {}: {} V, {} A, {} W", std::str::from_utf8(&sen.name).unwrap(), sen.voltage, sen.current, sen.power);
    }
}

fn prime_connection(port: &mut dyn SerialPort) {
    println!("Stopping previously started continuous TX");
    let config = ContTxStruct {
        enable: pmd::CONFIG_FALSE,
        timestamp_bytes: pmd::CONFIG_TIMESTAMP_NONE,
        adc_channels: pmd::CONFIG_MASK_NONE,
    };

    let config: [u8; size_of::<ContTxStruct>()] = unsafe { transmute(config) };

    match port.write(&config) {
        Ok(_) => println!("Configured device"),
        Err(e) => panic!("Error while writing to device: {}", e),
    }

    port.flush().unwrap();

    thread::sleep(Duration::from_millis(500));

    let mut scratch: Vec<u8> = Vec::new();
    let l = port.read_to_end(&mut scratch);

    println!("Remaining data: {}", std::str::from_utf8(&scratch).unwrap());
}

fn read_calibration(port: &mut dyn SerialPort, calibration: &mut [i8; pmd::PMD_ADC_CH_NUM]) {
    /* Firmware check */
    match port.write(&[pmd::CMD_READ_ID]) {
        Ok(_) => println!("Successfully wrote to device"),
        Err(e) => panic!("Error while writing to device: {}", e),
    }

    port.flush().unwrap();

    let mut id_buffer = [0u8; size_of::<DeviceIdStruct>()];

    match port.read_exact(&mut id_buffer) {
        Ok(_) => println!("Received {} bytes", size_of::<DeviceIdStruct>()),
        Err(e) => panic!("Error while reading from device: {}", e),
    }

    let device_id: DeviceIdStruct = unsafe {
        transmute(id_buffer)
    };

    println!("Running firmware version {}", device_id.firmware);

    /* Get calibration data */
    match port.write(&[pmd::CMD_READ_CONFIG]) {
        Ok(_) => println!("Reading config data"),
        Err(e) => panic!("Error while writing to device: {}", e),
    }

    port.flush().unwrap();

    if device_id.firmware < 6 {
        let mut config_buffer = [0u8; size_of::<ConfigStruct>()];
        match port.read_exact(&mut config_buffer) {
            Ok(_) => println!("Received config data"),
            Err(e) => panic!("Error while reading from device: {}", e),
        }
        let config: ConfigStruct = unsafe {
            transmute(config_buffer)
        };
        for i in 0..pmd::PMD_ADC_CH_NUM {
            calibration[i] = config.adc_offset[i];
        }
    }
    else {
        let mut config_buffer = [0u8; size_of::<ConfigStructV5>()];
        match port.read_exact(&mut config_buffer) {
            Ok(_) => println!("Received config data"),
            Err(e) => panic!("Error while reading from device: {}", e),
        }
        let config: ConfigStructV5 = unsafe {
            transmute(config_buffer)
        };
        for i in 0..pmd::PMD_ADC_CH_NUM {
            calibration[i] = config.adc_offset[i];
            // TODO what about gain offset?
        }
    }

    println!("Calibration data: {:?}", calibration);
}