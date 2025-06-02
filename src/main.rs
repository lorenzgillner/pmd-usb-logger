mod pmd;

use clap::{Arg, Command};
use csv::Writer;
use std::fs::File;
use std::io::{stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::pmd::{adjust_device_timestamp, PmdUsb};

fn main() {
    env_logger::init();

    /* Set up command line options */
    let args = Command::new("pmd-usb-logger")
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .value_name("PATH")
                .help("Serial port to use, e.g. /dev/ttyUSB0 or COM0")
                .default_value("/dev/ttyUSB0"),
        )
        .arg(
            Arg::new("speed")
                .short('s')
                .long("speed")
                .value_name("LEVEL")
                .help("Set the polling speed level (0 ... 3)")
                .default_value("0"),
        )
        .arg(
            Arg::new("timeout")
                .short('t')
                .long("timeout")
                .value_name("MILLISECONDS")
                .help("If speed is set to 1, set the timeout in milliseconds")
                .default_value("1000")
                .requires_if("1", "speed"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("out")
                .value_name("FILE")
                .help("Output file to write to (leave empty to write to STDOUT)")
                .num_args(0..=1), // At most one argument
        )
        .arg(
            Arg::new("until")
                .short('u')
                .long("until")
                .value_name("SECONDS")
                .help("Stop execution after the specified number of seconds")
                .default_value("0")
                .num_args(0..=1)
        )
        .get_matches();

    /* Dispatch command line options */
    let port_name = args.get_one::<String>("port").unwrap();
    let speed_level = args
        .get_one::<String>("speed")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let timeout = args
        .get_one::<String>("timeout")
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let output_file = args.get_one::<String>("output");
    let until = args
        .get_one::<String>("until")
        .unwrap()
        .parse::<u64>()
        .unwrap();

    /* Check serial port validity */
    if !check_port_validity(port_name) {
        println!("Could not open device: Invalid port name \"{}\"", port_name);
        std::process::exit(1);
    }

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
            return;
        }
        2 => pmd_usb.enable_cont_tx(),
        3 => {
            pmd_usb.bump_baud_rate();
            pmd_usb.enable_cont_tx();
        }
        _ if speed_level > 3 => {
            println!("Error: speed level should be between 0 and 3");
            std::process::exit(1);
        }
        _ => {}
    }

    /* Set up main loop */
    let running = Arc::new(AtomicBool::new(true));
    let rc = running.clone();
    let ru = running.clone();

    /* Set up interrupt handler */
    ctrlc::set_handler(move || {
        log::debug!("Ctrl-C pressed, stopping...");
        rc.store(false, Ordering::SeqCst); // Set the running flag to false
    })
    .expect("Error setting Ctrl-C handler");

    /* Print the CSV header */
    csv_writer
        .write_record([
            "timestamp",
            "PCIE1_V",
            "PCIE1_I",
            "PCIE2_V",
            "PCIE2_I",
            "EPS1_V",
            "EPS1_I",
            "EPS2_V",
            "EPS2_I",
        ])
        .expect("Failed to write header");

    /* Allocate vector for sensor values */
    let mut sensor_values: Vec<f64>;
    let mut timestamp: u128;

    /* If required, start the timeout thread */
    if until > 0 {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(until));
            ru.store(false, Ordering::SeqCst);
        });
    }

    /* Start the main loop */
    while running.load(Ordering::SeqCst) {
        if speed_level == 1 {
            let _sensor_values = pmd_usb.read_sensor_values();
            timestamp = get_host_timestamp();
            sensor_values = pmd_usb.convert_sensor_values(&_sensor_values);
            std::thread::sleep(Duration::from_millis(timeout));
        } else {
            let timed_adc_buffer = pmd_usb.read_cont_tx();
            let adc_buffer = timed_adc_buffer.buffer;
            // timestamp = timed_adc_buffer.timestamp as u128;
            timestamp = adjust_device_timestamp(timed_adc_buffer.timestamp);
            sensor_values = pmd_usb.convert_adc_values(&adc_buffer);
        }
        let sensor_values_export: Vec<String> =
            sensor_values.iter().map(|v| v.to_string()).collect();
        csv_writer
            .write_field(timestamp.to_string())
            .expect("Failed to write timestamp");
        csv_writer
            .write_record(sensor_values_export)
            .expect("Failed to write data record");
        csv_writer.flush().expect("Failed to flush CSV");
    }

    /* Clean up */
    match speed_level {
        2 => pmd_usb.disable_cont_tx(),
        3 => {
            pmd_usb.disable_cont_tx();
            pmd_usb.restore_baud_rate();
        }
        _ => {}
    }
}

fn check_port_validity(port_name: &str) -> bool {
    let available_ports = serialport::available_ports().unwrap();
    let is_valid_port = available_ports
        .iter()
        .any(|port_info| port_info.port_name == port_name);

    is_valid_port
}

fn get_host_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_micros()
}
