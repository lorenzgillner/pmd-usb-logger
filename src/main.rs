mod pmd;

use std::io::Read;
use clap::{Arg, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::pmd::PMD;

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
    check_port_validity(&port_name);

    /* Give the user some feedback */
    log::debug!("Selecting device {}", port_name);

    /* Connect to the PMD */
    let mut pmd_usb = PMD::new(&port_name);

    /* Set up device */
    pmd_usb.init();

    /* Set up main loop */
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    
    ctrlc::set_handler(move || {
        log::debug!("Ctrl-C pressed, stopping...");
        r.store(false, Ordering::SeqCst); // Set the running flag to false
    }).expect("Error setting Ctrl-C handler");
    
    pmd_usb.enable_cont_tx();
    
    while running.load(Ordering::SeqCst) {
        let adc_buffer = pmd_usb.read_cont_tx();
        let sensor_values = pmd_usb.convert_adc_values(&adc_buffer);
        println!("{:?}", sensor_values);
    }
    
    pmd_usb.disable_cont_tx();
}

fn check_port_validity(port_name: &str) {
    let available_ports = serialport::available_ports().unwrap();
    let is_valid_port = available_ports
        .iter()
        .any(|port_info| &port_info.port_name == port_name);

    if !is_valid_port {
        println!("Invalid port name: {}", port_name);
        std::process::exit(1);
    }
}