# Example code for fast reception of ElmorLabs PMD-USB measurement readings through serial interface
# Based on https://github.com/bjorntas/elmorlabs-pmd-usb-serial-interface/tree/main

import serial
import serial.tools.list_ports
import time
import pandas as pd
from ctypes import *

# data classes
class CalStruct(Structure):
    _fields_ = [
        ('AdcOffset', c_int8)
    ]

    def __str__(self):
        return f'Vendor {str(self.Vendor)} Product {str(self.Product)} Firmware {str(self.Firmware)}'


class DeviceIdStruct(Structure):
    _pack_ = 1
    _fields_ = [
        ('Vendor', c_uint8),
        ('Product', c_uint8),
        ('Firmware', c_uint8)
    ]

    def __str__(self):
        return f'Vendor {str(self.Vendor)} Product {str(self.Product)} Firmware {str(self.Firmware)}'


class PMD_USB_ConfigStruct(Structure):
    _pack_ = 2
    _fields_ = [
        ('Version', c_uint8),
        ('Crc', c_uint16),
        ('AdcOffset', c_int8*8),
        ('OledDisable', c_uint8),
        ('TimeoutCount', c_uint16),
        ('TimeoutAction', c_uint8),
        ('OledSpeed', c_uint8),
        ('RestartAdcFlag', c_uint8),
        ('CalFlag', c_uint8),
        ('UpdateConfigFlag', c_uint8),
        ('OledRotation', c_uint8),
        ('Averaging', c_uint8),
        ('rsvd', c_uint8*3)
    ]

    def __str__(self):
        return f'PMD-USB Config Struct Ver {str(self.Version)} Crc {str(self.Crc)}'

class PMD_USB_ConfigStruct_V5(Structure):
    _pack_ = 2
    _fields_ = [
        ('Version', c_uint8),
        ('Crc', c_uint16),
        ('AdcOffset', c_int8*8),
        ('OledDisable', c_uint8),
        ('TimeoutCount', c_uint16),
        ('TimeoutAction', c_uint8),
        ('OledSpeed', c_uint8),
        ('RestartAdcFlag', c_uint8),
        ('CalFlag', c_uint8),
        ('UpdateConfigFlag', c_uint8),
        ('OledRotation', c_uint8),
        ('Averaging', c_uint8),
        ('AdcGainOffset', c_int8*8),
        ('rsvd', c_uint8*3)
    ]

    def __str__(self):
        return f'PMD-USB Config Struct Ver {str(self.Version)} Crc {str(self.Crc)}'

# helper functions
def int8_from_adc(value):
    # check sign (8-bit)
    if(value & 0x80): # negative
        value = value - 0x100
    return value

def int16_from_adc(value):
    # check sign (12-bit)
    if(value & 0x800): # negative
        value = value - 0x1000
    return value

# settings

pmd_settings = {
    'port':'COM13', # change to your port
    'baudrate':115200, # default 115200
    'bytesize':8,
    'stopbits':1,
    'timeout':1
}

supported_baudrates = { 115200, 230400, 460800, 921600, 1500000, 2000000 }

list_all_windows_ports = True
save_to_csv = True
max_length = 1000

# storage of calibration data
cal_data = [0]*8

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

def read_calibration():

    global cal_data

    with serial.Serial(**pmd_settings) as ser:

        ser.write(b'\x01')
        ser.flush()
        buffer = ser.read(sizeof(DeviceIdStruct))

        # read firmware version
        id_struct = DeviceIdStruct.from_buffer_copy(buffer)

        # read config struct
        ser.write(b'\x04')
        ser.flush()

        if(id_struct.Firmware < 6):
            
            buffer = ser.read(sizeof(PMD_USB_ConfigStruct))
            config_struct = PMD_USB_ConfigStruct.from_buffer_copy(buffer)

            for i in range(0, 8):
                cal_data[i] = config_struct.AdcOffset[i]

        else:

            buffer = ser.read(sizeof(PMD_USB_ConfigStruct_V5))
            config_struct = PMD_USB_ConfigStruct_V5.from_buffer_copy(buffer)

            for i in range(0, 8):
                cal_data[i] = config_struct.AdcOffset[i]

        print('Calibration data: ', cal_data)


def set_baud_rate(baud_rate):

    print (f'Attemping to set baud rate to {baud_rate}')

    assert(baud_rate in supported_baudrates)

    # configure device for new baud rate
    with serial.Serial(**pmd_settings) as ser:

        ser.write(b'\x08') # cmd write config uart
    
        if(baud_rate == 115200):
            ser.write(b'\x00\xC2\x01\x00') # baud rate
        if(baud_rate == 230400):
            ser.write(b'\x00\x84\x03\x00')
        elif(baud_rate == 460800):
            ser.write(b'\x00\x08\x07\x00')
        elif(baud_rate == 921600):
            ser.write(b'\x00\x10\x0E\x00')
        elif(baud_rate == 1500000):
            ser.write(b'\x60\xE3\x16\x00')
        elif(baud_rate == 2000000):
            ser.write(b'\x80\x84\x1E\x00')

        ser.write(b'\x02\x00\x00\x00') # parity (2 = none)
        ser.write(b'\x00\x00\x00\x00') # data width (0 = 8 bits)
        ser.write(b'\x00\x00\x00\x00') # stop bits (0 = 1 bit)

        ser.flush()

    time.sleep(1)
    pmd_settings['baudrate'] = baud_rate

def continuous_data_rx(save_to_csv):

    global cal_data

    with serial.Serial(**pmd_settings) as ser:
        
        # define transferred data
        TIMESTAMP_BYTES = 0 # 1 = 8-bit, 2 = 16-bit, 4 = 32-bit
        ENABLE_PCIE1_VOLTAGE = 0 # 0 = disable, 1 = enable
        ENABLE_PCIE1_CURRENT = 0
        ENABLE_PCIE2_VOLTAGE = 0
        ENABLE_PCIE2_CURRENT = 0
        ENABLE_EPS1_VOLTAGE = 1
        ENABLE_EPS1_CURRENT = 1
        ENABLE_EPS2_VOLTAGE = 0
        ENABLE_EPS2_CURRENT = 0

        # build bitwise channel mask
        channel_mask = ENABLE_EPS2_CURRENT << 7 | ENABLE_EPS2_VOLTAGE << 6 | ENABLE_EPS1_CURRENT << 5 | ENABLE_EPS1_VOLTAGE << 4 | ENABLE_PCIE2_CURRENT << 3 | ENABLE_PCIE2_VOLTAGE << 2 | ENABLE_PCIE1_CURRENT << 1 | ENABLE_PCIE1_VOLTAGE << 0

        # empty buffer
        ser.read_all()

        # setup continuous data rx
        ser.write(b'\x07') # cmd write config cont tx
        ser.write(b'\x01') # 0 = disable, 1 = enable
        ser.write(int.to_bytes(TIMESTAMP_BYTES, 1, 'little')) # timestamp bytes 0-4
        ser.write(int.to_bytes(channel_mask, 1, 'little')) # bitwise channel mask
        ser.flush()

        # incoming buffer
        input_buffer = bytearray()
        value_buffer = []
        chunk_num_bytes = TIMESTAMP_BYTES + 2*(ENABLE_PCIE1_VOLTAGE + ENABLE_PCIE1_CURRENT + ENABLE_PCIE2_VOLTAGE + ENABLE_PCIE2_CURRENT + ENABLE_EPS1_VOLTAGE + ENABLE_EPS1_CURRENT + ENABLE_EPS2_VOLTAGE + ENABLE_EPS2_CURRENT)

        # speed measurement
        count = 0
        time_start = time.time_ns()*1.0/1e9
        
        while True:

            # read all data
            input_buffer.extend(ser.read_all())

            # read data chunks
            while(len(input_buffer) >= chunk_num_bytes):

                rx_buffer = input_buffer[0:chunk_num_bytes]
                input_buffer = input_buffer[chunk_num_bytes:]

                system_timestamp = time.time_ns()*1.0/1e9 # ns to s

                if(TIMESTAMP_BYTES == 1):
                    device_timestamp = (rx_buffer[0])*1.0/3e6 # 3 MHz timer on device
                elif(TIMESTAMP_BYTES == 2):
                    device_timestamp = (rx_buffer[1] << 8 | rx_buffer[0])*1.0/3e6
                elif(TIMESTAMP_BYTES == 4):
                    device_timestamp = (rx_buffer[3] << 24 | rx_buffer[2] << 16 | rx_buffer[1] << 8 | rx_buffer[0])*1.0/3e6
                else:
                    device_timestamp = 0

                rx_buffer_pos = TIMESTAMP_BYTES

                # default values
                pcie1_v = 0
                pcie1_i = 0
                pcie2_v = 0
                pcie2_i = 0
                eps1_v = 0
                eps1_i = 0
                eps2_v = 0
                eps2_i = 0

                # convert data
                if(ENABLE_PCIE1_VOLTAGE == 1):
                    pcie1_v = (int16_from_adc((rx_buffer[rx_buffer_pos + 1] << 8 | rx_buffer[rx_buffer_pos + 0]) >> 4) + int8_from_adc(cal_data[0])) * 0.007568
                    rx_buffer_pos += 2
                if(ENABLE_PCIE1_CURRENT == 1):
                    pcie1_i = (int16_from_adc((rx_buffer[rx_buffer_pos + 1] << 8 | rx_buffer[rx_buffer_pos + 0]) >> 4) + int8_from_adc(cal_data[1])) * 0.0488
                    rx_buffer_pos += 2
                if(ENABLE_PCIE2_VOLTAGE == 1):
                    pcie2_v = (int16_from_adc((rx_buffer[rx_buffer_pos + 1] << 8 | rx_buffer[rx_buffer_pos + 0]) >> 4) + int8_from_adc(cal_data[2])) * 0.007568
                    rx_buffer_pos += 2
                if(ENABLE_PCIE2_CURRENT == 1):
                    pcie2_i = (int16_from_adc((rx_buffer[rx_buffer_pos + 1] << 8 | rx_buffer[rx_buffer_pos + 0]) >> 4) + int8_from_adc(cal_data[3])) * 0.0488
                    rx_buffer_pos += 2
                if(ENABLE_EPS1_VOLTAGE == 1):
                    eps1_v = (int16_from_adc((rx_buffer[rx_buffer_pos + 1] << 8 | rx_buffer[rx_buffer_pos + 0]) >> 4) + int8_from_adc(cal_data[4])) * 0.007568
                    rx_buffer_pos += 2
                if(ENABLE_EPS1_CURRENT == 1):
                    eps1_i = (int16_from_adc((rx_buffer[rx_buffer_pos + 1] << 8 | rx_buffer[rx_buffer_pos + 0]) >> 4) + int8_from_adc(cal_data[5]))* 0.0488
                    rx_buffer_pos += 2
                if(ENABLE_EPS2_VOLTAGE == 1):
                    eps2_v = (int16_from_adc((rx_buffer[rx_buffer_pos + 1] << 8 | rx_buffer[rx_buffer_pos + 0]) >> 4) + int8_from_adc(cal_data[6])) * 0.007568
                    rx_buffer_pos += 2
                if(ENABLE_EPS2_CURRENT == 1):
                    eps2_i = (int16_from_adc((rx_buffer[rx_buffer_pos + 1] << 8 | rx_buffer[rx_buffer_pos + 0]) >> 4) + int8_from_adc(cal_data[7])) * 0.0488
                    rx_buffer_pos += 2
                
                pcie1_p = pcie1_v * pcie1_i
                pcie2_p = pcie2_v * pcie2_i
                eps1_p = eps1_v * eps1_i
                eps2_p = eps2_v * eps2_i
                    
                value_buffer.append(eps1_i)

                count += 1
                time_elapsed = system_timestamp - time_start
                if(time_elapsed >= 0.1): # 100ms
                    time_start = system_timestamp
                    print(f'Time: {system_timestamp:.6f} Samples: {count}', f'Min: {min(value_buffer):.3f}A', f'Avg: {sum(value_buffer)/len(value_buffer):.3f}A', f'Max: {max(value_buffer):.3f}A')
                    value_buffer = []
                    count = 0

if __name__ == '__main__':

    if list_all_windows_ports:
        ports = list(serial.tools.list_ports.comports())
        print('USB PORTS: ')
        for p in ports:
            print(p)
        print()

    prime_connection()

    check_connection()

    #set_baud_rate(115200) # only necessary to increase the sample rate

    read_calibration()
    
    continuous_data_rx(save_to_csv=False)

