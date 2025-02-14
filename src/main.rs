mod pmd;

use std::io::Read;
use clap::{Arg, Command};
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use bytemuck::cast_slice;
use serialport::SerialPort;
use crate::pmd::{disable_continuous_tx, AdcArray, PMD_ADC_BYTE_NUM, PMD_ADC_CH_NUM};

// struct ContinuousReader {
//     port: Arc<dyn SerialPort>,
//     buffer_size: usize,
// }

// impl Iterator for ContinuousReader {
//     type Item = Vec<u8>;
//     
//     fn next(&mut self) -> Option<Self::Item> {
//         match 
//     }
// }

fn main() {
    /* Set up command line options */
    let args = Command::new("pmd-usb-logger")
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .value_name("PORT")
                .help("Serial port to use, e.g. /dev/ttyUSB0 or COM0")
                .default_value("/dev/ttyUSB0"),
        )
        .arg(
            Arg::new("baudrate")
                .short('b')
                .long("baudrate")
                .value_name("BAUDRATE")
                .help("Baud rate to use, e.g. 115200")
                .default_value("115200"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose output")
                .action(clap::ArgAction::Count),
        )
        .get_matches();

    /* Dispatch command line options */
    let port_name = args.get_one::<String>("port").unwrap();
    let baudrate: u32 = args
        .get_one::<String>("baudrate")
        .unwrap()
        .parse()
        .expect("Invalid baud rate");
    // let log_level = match args.get_count("verbose") {
    //     0 => "warn",
    //     1 => "info",
    //     _ => "debug",
    // };

    env_logger::init();

    /* Check serial port validity */
    let available_ports = serialport::available_ports().unwrap();
    let is_valid_port = available_ports
        .iter()
        .any(|port_info| &port_info.port_name == port_name);

    if !is_valid_port {
        println!("Invalid port name: {}", port_name);
        std::process::exit(1);
    }

    /* Give the user some feedback */
    println!("Selecting device {} at baud rate {}", port_name, baudrate); // TODO debug!

    /* Initialize serial connection */
    let mut port = match serialport::new(port_name, baudrate)
        .timeout(Duration::from_secs(5))
        .data_bits(serialport::DataBits::Eight)
        .stop_bits(serialport::StopBits::One)
        .open()
    {
        Ok(p) => p,
        Err(e) => panic!("Unable to open serial port: {}", e),
    };

    /* "Global" calibration storage */
    let mut calibration: [[i8; 2]; pmd::PMD_ADC_CH_NUM] = [[0, 0]; pmd::PMD_ADC_CH_NUM];

    /* Stop previously started continuous TX */
    pmd::disable_continuous_tx(&mut *port);
    
    /* Send welcome message */
    pmd::welcome(&mut *port);
    
    /* Check sensors */
    pmd::read_sensors(&mut *port);

    /* Read and store device calibration parameters */
    pmd::read_calibration(&mut *port, &mut calibration);
    
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    
    ctrlc::set_handler(move || {
        log::debug!("Ctrl-C pressed, stopping...");
        r.store(false, Ordering::SeqCst); // Set the running flag to false
    }).expect("Error setting Ctrl-C handler");
    
    pmd::enable_continuous_tx(&mut *port, pmd::TimestampSize::None);
    
    let mut buffer = [0u8; PMD_ADC_BYTE_NUM];
    
    while running.load(Ordering::SeqCst) {
        match port.read_exact(&mut buffer) {
            Ok(_) => (),
            Err(e) => panic!("Error while reading from device: {}", e),
        }
    
        // TODO correctly interpret the returned bytes
        println!("{:#04X?}", buffer);
    }
    
    pmd::disable_continuous_tx(&mut *port);
    
    let adc_buffer = pmd::read_adc_buffer(&mut *port);
    println!("{:?}", adc_buffer);

    let sensor_data = pmd::read_sensor_values(&mut *port);
    println!("{:?}", sensor_data);
}
