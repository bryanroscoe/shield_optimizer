# Examples

## Launch a command on device

```rust no_run
use adb_client::{server::ADBServer, ADBDeviceExt};

let mut server = ADBServer::default();
let mut device = server.get_device().expect("cannot get device");
device.shell_command(&"df -h", Some(&mut std::io::stdout()), None);
```

## Push a file to the device

```rust no_run
use adb_client::server::ADBServer;
use std::net::Ipv4Addr;
use std::fs::File;
use std::path::Path;

let mut server = ADBServer::default();
let mut device = server.get_device().expect("cannot get device");
let mut input = File::open(Path::new("/tmp/file.txt")).expect("Cannot open file");
device.push(&mut input, "/data/local/tmp");
```
