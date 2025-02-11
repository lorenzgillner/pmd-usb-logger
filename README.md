# pmd-usb-logger

Link to product: https://elmorlabs.com/product/elmorlabs-pmd-usb-power-measurement-device-with-usb/

Based on:

* https://github.com/ElmorLabs/PMDLogger/tree/master
* https://github.com/bjorntas/elmorlabs-pmd-usb-serial-interface/tree/main
* https://github.com/ElmorLabs/pmd-usb-serial-interface-fast/tree/main

## PMD messages explained

| Code  | Description          |
|-------|----------------------|
| `0x0` | welcome message      |
| `0x1` | read device ID       |
| `0x2` | read sensors         |
| `0x3` | read values          |
| `0x4` | read config          |
| `0x6` | read ADC buffer      |
| `0x7` | write config cont tx |
| `0x8` | write config uart    |