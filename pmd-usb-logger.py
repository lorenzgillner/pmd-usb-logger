#!/usr/bin/python3

import argparse
import csv
import logging
import serial
import serial.tools.list_ports
import time
import datetime
import operator
from ctypes import *

import pmd


# ==========
#  Settings
# ==========

supported_baudrates = [115200, 230400, 460800, 921600, 1500000, 2000000]

pmd_settings = {
    "port": "/dev/ttyUSB0",
    "baudrate": supported_baudrates[0],
    "bytesize": serial.EIGHTBITS,
    "stopbits": serial.STOPBITS_ONE,
    "timeout": 1.0,
}

# storage for calibration data
cal_data = [0] * 8



# ==================
#  Helper functions
# ==================


def int8_from_adc(value):
    # check sign (8-bit)
    if value & 0x80:  # `value` is negative
        value -= 0x100
    return value


def int16_from_adc(value):
    # check sign (12-bit)
    if value & 0x800:  # `value` is negative
        value -= 0x1000
    return value


def enable_verbosity():
    global VERBOSE
    VERBOSE = True


# ===============
#  PMD functions
# ===============


def prime_connection():
    with serial.Serial(**pmd_settings) as ser:
        if VERBOSE:
            print("Stopping previously started continuous TX")

        # stop cont rx if already running
        ser.write(pmd.PMD_USB_WRITE_CONT_TX)    # cmd write config cont tx
        ser.write(PMD_USB_DISABLE)          # 0x0 = disable, 0x1 = enable
        ser.write(PMD_USB_TIMESTAMP_NONE)   # timestamp bytes 0
        ser.write(PMD_USB_MASK_NONE)        # bitwise channel mask
        ser.flush()

        # wait for command to execute
        time.sleep(1)

        # clear buffer
        ser.read_all()


def check_connection():
    with serial.Serial(**pmd_settings) as ser:
        # clear buffer
        ser.read_all()

        # check welcome message
        ser.write(PMD_USB_WELCOME)
        ser.flush()
        read_bytes = ser.read(18)
        if read_bytes != PMD_USB_RESPONSE:
            return False

        # check sensor struct
        ser.write(PMD_USB_READ_SENSORS)
        ser.flush()
        read_bytes = ser.read(48)

        if len(read_bytes) != sizeof(SensorStruct):
            return False

        return True


def read_sensors():
    with serial.Serial(**pmd_settings) as ser:
        # clear buffer
        ser.read_all()

        ser.write(PMD_USB_READ_SENSORS)
        ser.flush()
        buffer = ser.read(sizeof(SensorStruct))

        sensor_struct = SensorStruct.from_buffer_copy(buffer)


def read_calibration():
    global cal_data

    with serial.Serial(**pmd_settings) as ser:
        ser.write(PMD_USB_READ_ID)
        ser.flush()
        buffer = ser.read(sizeof(DeviceIdStruct))

        # read firmware version
        id_struct = DeviceIdStruct.from_buffer_copy(buffer)

        # read config struct
        ser.write(PMD_USB_READ_CONFIG)
        ser.flush()

        if id_struct.Firmware < 6:
            buffer = ser.read(sizeof(PMD_USB_ConfigStruct))
            config_struct = PMD_USB_ConfigStruct.from_buffer_copy(buffer)

            for i in range(0, 8):
                cal_data[i] = config_struct.AdcOffset[i]

        else:
            buffer = ser.read(sizeof(PMD_USB_ConfigStruct_V5))
            config_struct = PMD_USB_ConfigStruct_V5.from_buffer_copy(buffer)

            for i in range(0, 8):
                cal_data[i] = config_struct.AdcOffset[i]

        if VERBOSE:
            print("Running firmware version ", id_struct.Firmware)
            print("Calibration data: ", cal_data)


def set_baudrate(baudrate):
    assert baudrate in supported_baudrates

    # configure device for new baud rate
    with serial.Serial(**pmd_settings) as ser:
        ser.write(PMD_USB_WRITE_CONFIG_UART)

        def as_bytes(value):
            return int.to_bytes(value, length=4, byteorder="little")

        ser.write(as_bytes(baudrate))   # 32 bit baud rate
        ser.write(as_bytes(2))          # 32 bit parity (2 = none)
        ser.write(as_bytes(0))          # 32 bit data width (0 = 8 bits)
        ser.write(as_bytes(0))          # 32 bit stop bits (0 = 1 bit)

        ser.flush()

    time.sleep(1)

    pmd_settings["baudrate"] = baudrate

    if VERBOSE:
        print("Set baud rate to ", baudrate)


def read_data():
    global cal_data
    with serial.Serial(**pmd_settings) as ser:
        ser.write(PMD_USB_READ_VALUES)
        ser.flush()
        bytes_rx = ser.read(PMD_USB_SENSOR_NUM_BYTES)

        for ()



def continuous_data_rx():
    global cal_data

    device_clock_res = 3e6
    host_clock_res = 1e9

    with serial.Serial(**pmd_settings) as ser:
        # define transferred data
        timestamp_bytes = 4

        # empty buffer
        ser.read_all()

        # setup device-side continuous data tx
        ser.write(PMD_USB_WRITE_CONT_TX)
        ser.write(PMD_USB_ENABLE)
        ser.write(PMD_USB_TIMESTAMP_FULL)
        ser.write(PMD_USB_MASK_ALL)
        ser.flush()

        # incoming buffer
        input_buffer = bytearray()

        chunk_num_bytes = timestamp_bytes + 2 * sum(8)

        # speed measurement
        time_start = time.time_ns() / host_clock_res

        # RX loop
        while True:
            # read all data
            input_buffer.extend(ser.read_all())

            # read data chunks
            while len(input_buffer) >= chunk_num_bytes:
                rx_buffer = input_buffer[0:chunk_num_bytes]
                input_buffer = input_buffer[chunk_num_bytes:]

                # 3 MHz timer on device
                device_timestamp = (
                            rx_buffer[3] << 24
                            | rx_buffer[2] << 16
                            | rx_buffer[1] << 8
                            | rx_buffer[0]
                        ) / device_clock_res

                system_timestamp = time.time_ns() / host_clock_res

                rx_buffer_pos = timestamp_bytes

                # default values
                pcie1_v = 0
                pcie1_i = 0
                pcie1_p = 0
                pcie2_v = 0
                pcie2_i = 0
                pcie2_p = 0
                eps1_v = 0
                eps1_i = 0
                eps1_p = 0
                eps2_v = 0
                eps2_i = 0
                eps2_p = 0

                pcie1_v = (int16_from_adc(
                            (
                                rx_buffer[rx_buffer_pos + 1] << 8
                                | rx_buffer[rx_buffer_pos]
                            )
                            >> 4
                        )
                        + int8_from_adc(cal_data[0])
                    ) * 0.007568
                rx_buffer_pos += 2

                if channel_settings["PCIE1_CURRENT"]:
                    pcie1_i = (
                        int16_from_adc(
                            (
                                rx_buffer[rx_buffer_pos + 1] << 8
                                | rx_buffer[rx_buffer_pos]
                            )
                            >> 4
                        )
                        + int8_from_adc(cal_data[1])
                    ) * 0.0488
                    rx_buffer_pos += 2
                if channel_settings["PCIE2_VOLTAGE"]:
                    pcie2_v = (
                        int16_from_adc(
                            (
                                rx_buffer[rx_buffer_pos + 1] << 8
                                | rx_buffer[rx_buffer_pos]
                            )
                            >> 4
                        )
                        + int8_from_adc(cal_data[2])
                    ) * 0.007568
                    rx_buffer_pos += 2
                if channel_settings["PCIE2_CURRENT"]:
                    pcie2_i = (
                        int16_from_adc(
                            (
                                rx_buffer[rx_buffer_pos + 1] << 8
                                | rx_buffer[rx_buffer_pos]
                            )
                            >> 4
                        )
                        + int8_from_adc(cal_data[3])
                    ) * 0.0488
                    rx_buffer_pos += 2
                if channel_settings["EPS1_VOLTAGE"]:
                    eps1_v = (
                        int16_from_adc(
                            (
                                rx_buffer[rx_buffer_pos + 1] << 8
                                | rx_buffer[rx_buffer_pos]
                            )
                            >> 4
                        )
                        + int8_from_adc(cal_data[4])
                    ) * 0.007568
                    rx_buffer_pos += 2
                if  channel_settings["EPS1_CURRENT"]:
                    eps1_i = (
                        int16_from_adc(
                            (
                                rx_buffer[rx_buffer_pos + 1] << 8
                                | rx_buffer[rx_buffer_pos]
                            )
                            >> 4
                        )
                        + int8_from_adc(cal_data[5])
                    ) * 0.0488
                    rx_buffer_pos += 2
                if channel_settings["EPS2_VOLTAGE"]:
                    eps2_v = (
                        int16_from_adc(
                            (
                                rx_buffer[rx_buffer_pos + 1] << 8
                                | rx_buffer[rx_buffer_pos]
                            )
                            >> 4
                        )
                        + int8_from_adc(cal_data[6])
                    ) * 0.007568
                    rx_buffer_pos += 2
                if channel_settings["EPS2_CURRENT"]:
                    eps2_i = (
                        int16_from_adc(
                            (
                                rx_buffer[rx_buffer_pos + 1] << 8
                                | rx_buffer[rx_buffer_pos]
                            )
                            >> 4
                        )
                        + int8_from_adc(cal_data[7])
                    ) * 0.0488
                    rx_buffer_pos += 2


                pcie1_p = pcie1_v * pcie1_i
                pcie2_p = pcie2_v * pcie2_i
                eps1_p = eps1_v * eps1_i
                eps2_p = eps2_v * eps2_i

                value_buffer.append(eps1_i)

                count += 1
                time_elapsed = system_timestamp - time_start
                if time_elapsed >= 0.1:  # 100ms
                    time_start = system_timestamp
                    print(
                        f"Time: {system_timestamp:.6f} Samples: {count}",
                        f"Min: {min(value_buffer):.3f}A",
                        f"Avg: {sum(value_buffer)/len(value_buffer):.3f}A",
                        f"Max: {max(value_buffer):.3f}A",
                    )
                    value_buffer = []
                    count = 0


def configure_logging(args):
    if args.quiet:
        log_level = logging.ERROR
    elif args.verbose == 1:
        log_level = logging.INFO
    elif args.verbose > 1:
        log_level = logging.DEBUG
    else:
        log_level = logging.WARNING

    logging.basicConfig(level=log_level, format="%(asctime)s [%(levelname)s] %(message)s")


if __name__ == "__main__":
    # handle command line arguments
    parser = argparse.ArgumentParser(
        prog="pmd-usb-logger",
        description="Fast logging of ADC data from the ElmorLabs PMD-USB",
        usage="%(prog)s [options] output", # default output should be stdout
    )
    parser.add_argument("-p", "--port", default=pmd_settings["port"])
    # TODO parser.add_argument("-b", "--baudrate", action='set_baudrate')
    parser.add_argument("-v", "--verbose", action="count", default=0,
                        help="Increase verbosity level (use -vv for more details).")
    parser.add_argument("-q", "--quiet", action="store_true",
                        help="Suppress non-error output.")
    args = parser.parse_args()

    configure_logging(args)

    # find working baud rate, starting at the highest possible value
    device_response = False
    
    for baudrate in reversed(sorted(supported_baudrates)):
        pmd_settings["baudrate"] = baudrate

        prime_connection()

        if check_connection():
            logging.info(f"Successfully communicated with device at {baudrate} baud. Settings baud rate.")
            device_response = True
            break
        else:
            print(f"Unable to communicate with device at {baudrate} baud.")

    if not device_response:
        print("Unable to communicate with the device at any baud rate. Exiting.")
        exit(-1)

    # get calibration parameters from the device
    read_calibration()

    # start logging
    continuous_data_rx()

    # TODO exit gracefully
