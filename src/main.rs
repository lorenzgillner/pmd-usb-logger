mod pmd;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::io::{Write, stdout};
use std::fs::File;
use clap::{Arg, Command};
use csv::Writer;

use crate::pmd::PmdUsb;

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
        .arg(
            Arg::new("speed")
                .short('s')
                .long("speed")
                .value_name("SPEED_LEVEL")
                .help("Set the polling speed level")
                .default_value("0"),
        )
        .arg(
            Arg::new("output")
                .value_name("FILE")
                .help("Output file to write to (leave empty to write to STDOUT)")
                .num_args(0..=1) // At most one argument
        )
        .get_matches();

    /* Dispatch command line options */
    let port_name = args.get_one::<String>("port").unwrap();
    let speed_level = args.get_one::<String>("speed").unwrap().parse::<u32>().unwrap();
    let output_file = args.get_one::<String>("output");

    /* Check serial port validity */
    check_port_validity(port_name);

    /* Give the user some feedback */
    log::debug!("Selecting device {}", port_name);

    /* Choose either an output file or STDOUT */
    let writer: Box<dyn Write> = match output_file {
        Some(filename) => Box::new(File::create(filename).expect("Failed to create file")),
        None => Box::new(stdout()),
    };
    
    /* Create a CSV writer from the boxed writer */
    let mut csv_writer = Writer::from_writer(writer);

    /* Connect to the PMD */
    let mut pmd_usb = PmdUsb::new(port_name);

    /* Set up device */
    pmd_usb.init();

    /* Prepare main loop depending on speed level */
    match speed_level {
        0 => {
            let sensors = pmd_usb.read_sensors();
            println!("{:?}", sensors.sensor);
            return
        },
        2 => pmd_usb.enable_cont_tx(),
        3 => {
            pmd_usb.bump_baud_rate();
            pmd_usb.enable_cont_tx();
        },
        _ if speed_level > 3 => {
            println!("Error: speed level should be between 0 and 3");
            std::process::exit(1);
        },
        _ => {},
    }

    /* Set up main loop */
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    /* Set up interrupt handler */
    ctrlc::set_handler(move || {
        log::debug!("Ctrl-C pressed, stopping...");
        r.store(false, Ordering::SeqCst); // Set the running flag to false
    }).expect("Error setting Ctrl-C handler");

    /* Allocate vector for sensor values */
    let mut sensor_values: Vec<f64>;

    /* Start the main loop */
    while running.load(Ordering::SeqCst) {
        if speed_level == 1 {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let _sensor_values = pmd_usb.read_sensor_values();
            sensor_values = pmd_usb.convert_sensor_values(&_sensor_values)
        } else {
            let adc_buffer = pmd_usb.read_cont_tx();
            sensor_values = pmd_usb.convert_adc_values(&adc_buffer);
        }
        let sensor_values_export: Vec<String> = sensor_values.iter().map(|v| v.to_string()).collect();
        csv_writer.write_record(sensor_values_export).expect("Failed to write CSV");
    }

    /* Clean up */
    match speed_level {
        2 => pmd_usb.disable_cont_tx(),
        3 => {
            pmd_usb.disable_cont_tx();
            pmd_usb.restore_baud_rate();
        },
        _ => {},
    }
}

fn check_port_validity(port_name: &str) {
    let available_ports = serialport::available_ports().unwrap();
    let is_valid_port = available_ports
        .iter()
        .any(|port_info| port_info.port_name == port_name);

    if !is_valid_port {
        println!("Error: Invalid port name \"{}\"", port_name);
        std::process::exit(1);
    }
}
