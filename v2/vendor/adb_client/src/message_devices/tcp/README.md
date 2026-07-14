# Examples

## Get a shell from device

```rust no_run
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use adb_client::{tcp::ADBTcpDevice, ADBDeviceExt};

let mut device = ADBTcpDevice::new((IpAddr::from([192, 168, 0, 10]), 43210)).expect("cannot find device");
device.shell(&mut std::io::stdin(), Box::new(std::io::stdout()));
```
