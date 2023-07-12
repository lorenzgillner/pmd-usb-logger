# Example code for fast reception of ElmorLabs PMD-USB measurement readings through serial interface
# Based on https://github.com/bjorntas/elmorlabs-pmd-usb-serial-interface/tree/main

import serial
import serial.tools.list_ports
import time
import pandas as pd

# settings

pmd_settings = {
    'port':'COM5',
    'baudrate':115200,
    'bytesize':8,
    'stopbits':1,
    'timeout':1
}

supported_baudrates = { 115200, 460800 }

list_all_windows_ports = True
save_to_csv = True
max_length = 1000

def prime_connection():

    with serial.Serial(**pmd_settings) as ser:

        # stop cont rx if already running
        ser.write(b'\x07') # cmd write config cont tx
        ser.write(b'\x00') # 0 = disable, 1 = enable
        ser.write(b'\x00') # timestamp bytes 0-4
        ser.write(b'\x00') # bitwise channel mask
        ser.flush()
        time.sleep(1)
        ser.read_all()

def check_connection():
    with serial.Serial(**pmd_settings) as ser:

        # b'\x00'   welcome message
        # b'\x01'   ID
        # b'\x02'   read sensors
        # b'\x03'   read values
        # b'\x04'   read config
        # b'\x06'   read ADC buffer
        # b'\x07'   write config cont tx
        # b'\x08'   write config uart

        # check welcome message
        ser.write(b'\x00')
        ser.flush()
        read_bytes = ser.read(18)
        assert read_bytes == b'ElmorLabs PMD-USB'

        # check sensor struct
        ser.write(b'\x02')
        ser.flush()
        read_bytes = ser.read(100)
        print('Struct: ', read_bytes)

def set_baud_rate(baud_rate):

    assert(baud_rate in supported_baudrates)

    '''baud_bytes = int.to_bytes(baud_rate, 4, 'little')
    parity_bytes = int.to_bytes(2, 4, 'little') # no parity
    datawidth_bytes = int.to_bytes(0, 4, 'little') # 8 bits
    stopbits_bytes = int.to_bytes(0, 4, 'little') # 1 bit'''

    # configure device for new baud rate
    with serial.Serial(**pmd_settings) as ser:

        ser.write(b'\x08') # cmd write config uart
        
        '''ser.write(baud_bytes[0])
        ser.write(baud_bytes[1])
        ser.write(baud_bytes[2])
        ser.write(baud_bytes[3])
        ser.write(parity_bytes[0])
        ser.write(parity_bytes[1])
        ser.write(parity_bytes[2])
        ser.write(parity_bytes[3])
        ser.write(datawidth_bytes[0])
        ser.write(datawidth_bytes[1])
        ser.write(datawidth_bytes[2])
        ser.write(datawidth_bytes[3])
        ser.write(stopbits_bytes[0])
        ser.write(stopbits_bytes[1])
        ser.write(stopbits_bytes[2])
        ser.write(stopbits_bytes[3])'''

        if(baud_rate == 115200):
            ser.write(b'\x00\xC2\x01\x00') # baud rate
        elif(baud_rate == 460800):
            ser.write(b'\x00\x07\x80\x00')

        ser.write(b'\x02\x00\x00\x00') # parity (2 = none)
        ser.write(b'\x00\x00\x00\x00') # data width (0 = 8 bits)
        ser.write(b'\x00\x00\x00\x00') # stop bits (0 = 1 bit)

        ser.flush()

    time.sleep(1)
    pmd_settings['baudrate'] = baud_rate

def int16_from_adc(value):
    # check sign (12-bit)
    if(value & 0x800):
        # negative
        value = value - 0x1000

    return value

def continuous_data_rx(save_to_csv):

    with serial.Serial(**pmd_settings) as ser:

        # setup continuous data rx
        ser.write(b'\x07') # cmd write config cont tx
        ser.write(b'\x01') # 0 = disable, 1 = enable
        ser.write(b'\x00') # timestamp bytes 0-4
        ser.write(b'\xFF') # bitwise channel mask
        ser.flush()
        
        while True:
            
            # read data
            rx_buffer = ser.read(4*2*2) # 4 channels * 2 values V/I * 2 bytes per value
            timestamp = time.time_ns()

            # convert data
            pcie1_v = int16_from_adc((rx_buffer[1] << 8 | rx_buffer[0]) >> 4) * 0.007568
            pcie1_i = int16_from_adc((rx_buffer[3] << 8 | rx_buffer[2]) >> 4) * 0.0488
            pcie1_p = pcie1_v * pcie1_i
            pcie2_v = int16_from_adc((rx_buffer[5] << 8 | rx_buffer[4]) >> 4) * 0.007568
            pcie2_i = int16_from_adc((rx_buffer[7] << 8 | rx_buffer[6]) >> 4) * 0.0488
            pcie2_p = pcie2_v * pcie2_i
            eps1_v = int16_from_adc((rx_buffer[9] << 8 | rx_buffer[8]) >> 4) * 0.007568
            eps1_i = int16_from_adc((rx_buffer[11] << 8 | rx_buffer[10]) >> 4) * 0.0488
            eps1_p = eps1_v * eps1_i
            eps2_v = int16_from_adc((rx_buffer[13] << 8 | rx_buffer[12]) >> 4) * 0.007568
            eps2_i = int16_from_adc((rx_buffer[15] << 8 | rx_buffer[14]) >> 4) * 0.0488
            eps2_p = eps2_v * eps2_i

            gpu_power = pcie1_p + pcie2_p
            cpu_power = eps1_p + eps2_p
            total_power = gpu_power + cpu_power

            # save data
            print('Time: ', timestamp, 'PCIE1_V: ', pcie1_v, 'V')

def continuous_data_rx_single(save_to_csv):

    with serial.Serial(**pmd_settings) as ser:

        # setup continuous data rx
        ser.write(b'\x07') # cmd write config cont tx
        ser.write(b'\x01') # 0 = disable, 1 = enable
        ser.write(b'\x02') # timestamp bytes 0-4
        ser.write(b'\x03') # bitwise channel mask (only PCIE1 Voltage and Current)
        ser.flush()

        while True:

            # read data
            rx_buffer = ser.read(2 + 1*2*2) # 2 timestamp bytes + 1 channels * 2 values V/I * 2 bytes per value
            device_timestamp = (rx_buffer[1] << 8 | rx_buffer[0])*1.0/3e6 # 3 MHz timer on device
            system_timestamp = time.time_ns()*1.0/1e9 # ns to s

            # convert data
            pcie1_v = int16_from_adc((rx_buffer[3] << 8 | rx_buffer[2]) >> 4) * 0.007568
            pcie1_i = int16_from_adc((rx_buffer[5] << 8 | rx_buffer[4]) >> 4) * 0.0488
            pcie1_p = pcie1_v * pcie1_i

            # save data
            print('PCIE1 Time: ', system_timestamp, ' ', device_timestamp, ' ', pcie1_v, 'V', ' ', pcie1_i, 'A', ' ', pcie1_p, 'W')

if __name__ == '__main__':

    if list_all_windows_ports:
        ports = list(serial.tools.list_ports.comports())
        print('USB PORTS: ')
        for p in ports:
            print(p)
        print()

    prime_connection()

    check_connection()

    #set_baud_rate(460800)

    #check_connection()
    
    continuous_data_rx_single(save_to_csv=False)
