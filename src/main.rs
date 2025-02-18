mod pmd;

use std::io::Read;
use clap::{Arg, Command};
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use bytemuck::cast_slice;
use serialport::SerialPort;
use crate::pmd::read_continuous_tx;

const BAUDRATE: u32 = 115200;

fn main() {
    env_logger::init();
    
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
        .get_matches();

    /* Dispatch command line options */
    let port_name = args.get_one::<String>("port").unwrap();

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
    log::debug!("Selecting device {}", port_name);

    /* Open serial port connection */
    let mut port = match serialport::new(port_name, BAUDRATE)
        .timeout(Duration::from_secs(5))
        .data_bits(serialport::DataBits::Eight)
        .stop_bits(serialport::StopBits::One)
        .parity(serialport::Parity::None)
        // TODO what about RTS/DTR?
        .open()
    {
        Ok(p) => p,
        Err(e) => panic!("Unable to open serial port: {}", e),
    };

    /* "Global" calibration storage */
    // let mut calibration: [i8; pmd::PMD_ADC_CH_NUM] = [0; pmd::PMD_ADC_CH_NUM];

    /* Stop previously started continuous TX */
    pmd::disable_continuous_tx(&mut *port);
    
    /* Send welcome message */
    pmd::welcome(&mut *port);
        
    // /* Check sensors */
    pmd::read_sensors(&mut *port);

    /* Read and store device calibration parameters */
    // pmd::read_calibration(&mut *port, &mut calibration);
    
    /* Set up main loop */
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    
    ctrlc::set_handler(move || {
        log::debug!("Ctrl-C pressed, stopping...");
        r.store(false, Ordering::SeqCst); // Set the running flag to false
    }).expect("Error setting Ctrl-C handler");
    
    // FYI: this must be an array of u8, because that's what port.read() expects
    let mut buffer = [0u8; pmd::PMD_ADC_BYTE_NUM];
    
    pmd::enable_continuous_tx(&mut *port);
    
    // FYI: running.load(...) just reads the boolean value 
    while running.load(Ordering::SeqCst) {
        let adc_buffer = read_continuous_tx(port.as_mut());
        let adc_buffer = pmd::convert_adc_values(&adc_buffer);
        
        // 12 bit LE value inside a 16 bit LE?
        // adc_buffer[0] = ((buffer[1] as u16) << 8 | (buffer[0] as u16)) >> 4;
        // adc_buffer[1] = ((buffer[3] as u16) << 8 | (buffer[2] as u16)) >> 4;
        // adc_buffer[2] = ((buffer[5] as u16) << 8 | (buffer[4] as u16)) >> 4;
        // adc_buffer[3] = ((buffer[7] as u16) << 8 | (buffer[6] as u16)) >> 4;
        // adc_buffer[4] = ((buffer[9] as u16) << 8 | (buffer[8] as u16)) >> 4;
        // adc_buffer[5] = ((buffer[11] as u16) << 8 | (buffer[10] as u16)) >> 4;
        // adc_buffer[6] = ((buffer[13] as u16) << 8 | (buffer[12] as u16)) >> 4;
        // adc_buffer[7] = ((buffer[15] as u16) << 8 | (buffer[14] as u16)) >> 4;
        
        println!("{:?}", adc_buffer);
    }
    
    pmd::disable_continuous_tx(&mut *port);
    
    // let adc_buffer = pmd::read_adc_buffer(&mut *port);
    // println!("{:?}", adc_buffer);
    // 
    // let sensor_data = pmd::read_sensor_values(&mut *port);
    // println!("{:?}", sensor_data);
}
