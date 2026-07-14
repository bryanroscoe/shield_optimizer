# Examples

## Launch a command on device

```rust no_run
use adb_client::{usb::ADBUSBDevice, ADBDeviceExt};

let vendor_id = 0x04e8;
let product_id = 0x6860;
let mut device = ADBUSBDevice::new(vendor_id, product_id).expect("cannot find device");
device.shell_command(&"df -h", Some(&mut std::io::stdout()), None);
```

## Push a file to the device

```rust no_run
use adb_client::{usb::ADBUSBDevice, ADBDeviceExt};
use std::fs::File;
use std::path::Path;

let vendor_id = 0x04e8;
let product_id = 0x6860;
let mut device = ADBUSBDevice::new(vendor_id, product_id).expect("cannot find device");
let mut input = File::open(Path::new("/tmp/file.txt")).expect("Cannot open file");
device.push(&mut input, &"/data/local/tmp");
```
