from ctypes import *

PMD_USB_WELCOME = b"\x00"
PMD_USB_READ_ID = b"\x01"
PMD_USB_READ_SENSORS = b"\x02"
PMD_USB_READ_VALUES = b"\x03"
PMD_USB_READ_CONFIG = b"\x04"
PMD_USB_READ_ADC_BUFFER = b"\x06"
PMD_USB_WRITE_CONT_TX = b"\x07"
PMD_USB_WRITE_CONFIG_UART = b"\x08"
PMD_USB_ENABLE = b"\x01"
PMD_USB_DISABLE = b"\x00"
PMD_USB_MASK_NONE = b"\x00"
PMD_USB_MASK_ALL = b"\xff"
PMD_USB_TIMESTAMP_NONE = b"\x00"
PMD_USB_TIMESTAMP_FULL = b"\x04"

PMD_USB_RESPONSE = b"ElmorLabs PMD-USB"

PMD_USB_SENSOR_NUM = 4
PMD_USB_SENSOR_NUM_BYTES = 16
PMD_USB_SENSOR_NAME_LEN = 6

class DeviceIdStruct(Structure):
    _pack_ = 1
    _fields_ = [("Vendor", c_uint8), ("Product", c_uint8), ("Firmware", c_uint8)]

    def __str__(self):
        return f"Vendor {str(self.Vendor)} Product {str(self.Product)} Firmware {str(self.Firmware)}"


class ConfigStruct(Structure):
    _pack_ = 2
    _fields_ = [
        ("Version", c_uint8),
        ("Crc", c_uint16),
        ("AdcOffset", c_int8 * 8),
        ("OledDisable", c_uint8),
        ("TimeoutCount", c_uint16),
        ("TimeoutAction", c_uint8),
        ("OledSpeed", c_uint8),
        ("RestartAdcFlag", c_uint8),
        ("CalFlag", c_uint8),
        ("UpdateConfigFlag", c_uint8),
        ("OledRotation", c_uint8),
        ("Averaging", c_uint8),
        ("rsvd", c_uint8 * 3),
    ]

    def __str__(self):
        return f"PMD-USB Config Struct Ver {str(self.Version)} Crc {str(self.Crc)}"


class ConfigStructV5(Structure):
    _pack_ = 2
    _fields_ = [
        ("Version", c_uint8),
        ("Crc", c_uint16),
        ("AdcOffset", c_int8 * 8),
        ("OledDisable", c_uint8),
        ("TimeoutCount", c_uint16),
        ("TimeoutAction", c_uint8),
        ("OledSpeed", c_uint8),
        ("RestartAdcFlag", c_uint8),
        ("CalFlag", c_uint8),
        ("UpdateConfigFlag", c_uint8),
        ("OledRotation", c_uint8),
        ("Averaging", c_uint8),
        ("AdcGainOffset", c_int8 * 8),
        ("rsvd", c_uint8 * 3),
    ]

    def __str__(self):
        return f"PMD-USB Config Struct Ver {str(self.Version)} Crc {str(self.Crc)}"


class ReadingStruct(Structure):
    _pack_ = 2
    _fields_ = [
        ("Name", c_char * PMD_USB_SENSOR_NAME_LEN),
        ("Voltage", c_uint16),
        ("Current", c_uint16),
        ("Power", c_uint16),
    ]

    def __str__(self):
        return f"Sender '{str(self.Name)}'"


class SensorStruct(Structure):
    _pack_ = 2
    _fields_ = [
        ("Sensor", ReadingStruct * PMD_USB_SENSOR_NUM),
    ]

    def __str__(self):
        return f"Sensor '{str(self.Sensor)}'"