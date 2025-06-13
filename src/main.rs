mod pmd;

use crate::pmd::{adjust_device_timestamp, PmdUsb, SensorValues};
use clap::{Arg, Command};
use csv::Writer;
use std::fs::File;
use std::io::{stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

struct Config {
    speed_level: u8,
    interval: Duration,
    timeout: Duration,
}

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
            Arg::new("interval")
                .short('i')
                .long("interval")
                .value_name("MILLISECONDS")
                .help("If option speed is set to 1, set the polling interval (min. 5 ms)")
                .default_value("1000")
                .requires_if("1", "speed"),
        )
        .arg(
            Arg::new("timeout")
                .short('t')
                .long("timeout")
                .value_name("SECONDS")
                .help("Stop execution after the specified number of seconds")
                .default_value("0")
                .num_args(0..=1),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Output file to write to (leave empty to write to STDOUT)")
                .num_args(0..=1), // At most one argument
        )
        .get_matches();

    /* Dispatch command line options */
    let port_name = args.get_one::<String>("port").unwrap();
    let output = args.get_one::<String>("output").cloned();

    let config = Config {
        speed_level: args
            .get_one::<String>("speed")
            .unwrap()
            .parse::<u8>()
            .unwrap(),
        interval: Duration::from_millis(
            args.get_one::<String>("interval")
                .unwrap()
                .parse::<u64>()
                .unwrap(),
        ),
        timeout: Duration::from_secs(
            args.get_one::<String>("timeout")
                .unwrap()
                .parse::<u64>()
                .unwrap(),
        ),
    };

    /* Check serial port validity */
    if !check_port_validity(port_name) {
        println!("Could not open device: Invalid port name \"{}\"", port_name);
        std::process::exit(1);
    }

    /* Give the user some feedback */
    log::debug!("Selecting device {}", port_name);

    /* Connect to the PMD */
    let mut pmd_usb = PmdUsb::new(port_name);

    /* Set up device */
    pmd_usb.init();

    /* Prepare main loop depending on speed level */
    match config.speed_level {
        /* At this speed level, we simply print once and exit */
        0 => {
            let sensors = pmd_usb.read_sensors();
            println!("{:?}", sensors.sensor);
            return;
        }
        /* Prepare for continuous TX */
        2 => pmd_usb.enable_cont_tx(),
        3 => {
            pmd_usb.bump_baud_rate();
            pmd_usb.enable_cont_tx();
        }
        /* Speed level out of range */
        _ if config.speed_level > 3 => {
            println!("Error: speed level should be between 0 and 3");
            std::process::exit(1);
        }
        _ => {}
    }

    /* Set up main loop */
    let running = Arc::new(AtomicBool::new(true));
    let running_c = running.clone(); // ctrl+c
    let running_t = running.clone(); // timeout
    let running_w = running.clone(); // writer

    /* Set up interrupt handler */
    ctrlc::set_handler(move || {
        log::debug!("Ctrl-C pressed, stopping...");
        running_c.store(false, Ordering::SeqCst); // Set the running flag to false
    })
    .expect("Error setting Ctrl-C handler");

    /* Allocate sensor data */
    let mut timestamp: u128;
    let mut sensor_values: SensorValues;

    let (tx, rx) = channel::<(u128, SensorValues)>();

    /* Create a new thread for writing the output file */
    let writer_handle = thread::spawn(move || {
        log_to_csv(output, rx, running_w);
    });

    /* Switch polling method based on speed level */
    let read_pmd = match config.speed_level {
        1 => read_pmd_slow,
        _ => read_pmd_fast,
    };

    /* If required, start the timeout thread */
    let timeout_handle = if !config.timeout.is_zero() {
        Some(thread::spawn(move || {
            thread::sleep(config.timeout);
            running_t.store(false, Ordering::SeqCst);
        }))
    } else {
        None
    };

    /* Start the main loop */
    while running.load(Ordering::SeqCst) {
        /* Read sensor values depending on the current polling method */
        (timestamp, sensor_values) = read_pmd(&mut pmd_usb, &config);

        /* Send current sensor values to the writer */
        tx.send((timestamp, sensor_values)).unwrap();
    }

    /* Join the timeout thread, if possible */
    if let Some(handle) = timeout_handle {
        handle.join().expect("Failed to join timeout thread");
    }

    /* Join the CSV writer */
    writer_handle.join().expect("Failed to join writer thread");

    /* Reset the device */
    match config.speed_level {
        2 => pmd_usb.disable_cont_tx(),
        3 => {
            pmd_usb.disable_cont_tx();
            pmd_usb.restore_baud_rate();
        }
        _ => {}
    }
}

fn read_pmd_slow(pmd_usb: &mut PmdUsb, config: &Config) -> (u128, SensorValues) {
    let start = std::time::Instant::now();
    let _sensor_values = pmd_usb.read_sensor_values();
    let elapsed = start.elapsed();
    println!("{}", elapsed.as_micros());
    let timestamp = get_host_timestamp();
    let sensor_values = pmd_usb.convert_sensor_values(&_sensor_values);
    thread::sleep(if config.interval > elapsed {
        config.interval - elapsed
    } else {
        Duration::new(0, 0)
    });
    (timestamp, sensor_values)
}

fn read_pmd_fast(pmd_usb: &mut PmdUsb, config: &Config) -> (u128, SensorValues) {
    let timed_adc_buffer = pmd_usb.read_cont_tx();
    let adc_buffer = timed_adc_buffer.buffer;
    let timestamp = adjust_device_timestamp(timed_adc_buffer.timestamp);
    let sensor_values = pmd_usb.convert_adc_values(&adc_buffer);
    (timestamp, sensor_values)
}

fn log_to_csv(
    output: Option<String>,
    rx: Receiver<(u128, SensorValues)>,
    running: Arc<AtomicBool>,
) {
    /* Choose either an output file or STDOUT */
    let sink: Box<dyn Write> = match output {
        Some(path) => Box::new(File::create(path).expect("Failed to create output file")),
        None => Box::new(stdout()),
    };

    /* Create a new CSV file writer from the sink */
    let mut csv_writer = Writer::from_writer(sink);

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
        .expect("Failed to write CSV header");

    let mut timestamp: u128;
    let mut sensor_values: SensorValues;

    while running.load(Ordering::SeqCst) {
        (timestamp, sensor_values) = rx.recv().unwrap();
        let sensor_values_string: Vec<String> =
            sensor_values.iter().map(|v| v.to_string()).collect();
        csv_writer
            .write_field(timestamp.to_string())
            .expect("Failed to write timestamp");
        csv_writer
            .write_record(sensor_values_string)
            .expect("Failed to write CSV record");
        csv_writer.flush().expect("Failed to flush CSV writer");
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
