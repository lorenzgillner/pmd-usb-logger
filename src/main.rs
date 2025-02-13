mod pmd;

use std::io::{Read, Write};
use std::time::Duration;
use clap::{Arg, Command};
// use env_logger;
use serialport;

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
        .arg(Arg::new("prime")
            .short('P')
            .long("prime")
            .help("Terminate potentially running continuous TX")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("verbose")
            .short('v')
            .long("verbose")
            .help("Enable verbose output")
            .action(clap::ArgAction::Count))
    .get_matches();

    /* Dispatch command line options */
    let port_name = args.get_one::<String>("port").unwrap();
    let baudrate: u32 = args.get_one::<String>("baudrate").unwrap().parse().expect("Invalid baud rate");
    let prime: bool = args.get_flag("prime");
    // let log_level = match args.get_count("verbose") {
    //     0 => "warn",
    //     1 => "info",
    //     _ => "debug",
    // };

    /* Configure verbosity level */
    // env::set_var("RUST_LOG", log_level);
    // env_logger::Builder::new()
    //     .format(|buf, record| {
    //         writeln!(buf, "{} [{}]: {}", Local::now().to_string(), record.level(), record.args())
    //     })
    //     .init();

    /* Check serial port validity */
    let available_ports = serialport::available_ports().unwrap();
    let is_valid_port = available_ports.iter().any(|port_info| &port_info.port_name == port_name);

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
        .open() {
        Ok(p) => p,
        Err(e) => panic!("Unable to open serial port: {}", e),
    };

    /* "Global" calibration storage */
    let mut calibration: [[i8; 2]; pmd::PMD_ADC_CH_NUM] = [[0, 0]; pmd::PMD_ADC_CH_NUM];

    /* Send welcome message */
    pmd::welcome(&mut *port);

    /* Check sensors */
    pmd::read_sensors(&mut *port);

    /* Stop previously started continuous TX */
    if prime {
        pmd::prime_connection(&mut *port);
    }

    /* Read and store device calibration parameters */
    pmd::read_calibration(&mut *port, &mut calibration);

    let sensor_data = pmd::read_sensor_values(&mut *port);
    println!("{:?}", sensor_data);

    
}