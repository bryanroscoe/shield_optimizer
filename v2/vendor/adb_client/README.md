# `adb_client`

[![MIT licensed](https://img.shields.io/crates/l/adb_client.svg)](./LICENSE-MIT)
[![Documentation](https://docs.rs/adb_client/badge.svg)](https://docs.rs/adb_client)
[![Crates.io Total Downloads](https://img.shields.io/crates/d/adb_client)](https://crates.io/crates/adb_client)
![MSRV](https://img.shields.io/crates/msrv/adb_client)

Rust library implementing ADB protocol.

## Installation

Add `adb_client` crate as a dependency by simply adding it to your `Cargo.toml`:

```toml
[dependencies]
adb_client = "*"
```

## Crate features

|    Feature    |                   Description                   | Default? |
| :-----------: | :---------------------------------------------: | :------: |
| `framebuffer` |      Enables _framebuffer_-related methods      |   Yes    |
|    `mdns`     | Enables mDNS device discovery on local network. |    No    |
|     `usb`     |     Enables interactions with USB devices.      |    No    |

To deactivate some default features you can use the `default-features = false` option in your `Cargo.toml` file and manually specify the features you want to activate:

```toml
[dependencies]
adb_client = { version = "*", default-features = false, features = ["mdns", "usb"] }
```

## Examples

Usage examples can be found in the `examples/` directory of this repository.

Some example are also provided in the various `README.md` files of modules.

## Benchmarks

Benchmarks run on `v2.0.6`, on a **Samsung S10 SM-G973F** device and an **Intel i7-1265U** CPU laptop

### `ADBServerDevice` push vs `adb push`

`ADBServerDevice` performs all operations by using adb server as a bridge.

| File size | Sample size | `ADBServerDevice` |   `adb`   |               Difference               |
| :-------: | :---------: | :---------------: | :-------: | :------------------------------------: |
|   10 MB   |     100     |     350,79 ms     | 356,30 ms | <div style="color:green">-1,57 %</div> |
|  500 MB   |     50      |      15,60 s      |  15,64 s  | <div style="color:green">-0,25 %</div> |
|   1 GB    |     20      |      31,09 s      |  31,12 s  | <div style="color:green">-0,10 %</div> |
