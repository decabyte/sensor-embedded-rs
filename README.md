# Sensor Embedded Rust

Embedded firmware component of the Sensor Platform ecosystem, designed to run on ESP32-C6 microcontrollers. This project provides the edge computing foundation for sensor data collection and wireless communication in distributed sensor networks.

## 🏗️ Ecosystem Context

This embedded component is part of the broader [Sensor Platform](../sensor-platform-rs/) architecture:

```plain
┌─────────────────────────────────────────────────────────────┐
│                     Sensor Platform                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────┐         ┌──────────┐         ┌──────────┐     │
│  │   CLI    │◄────────┤ Service  │◄────────┤ Embedded │     │
│  │  (TUI)   │  HTTP   │  (REST)  │  Data   │  (ESP32) │     │
│  └──────────┘         └──────────┘         └──────────┘     │
│       │                  │    │                  │          │
│       └──────────────────┴────┴──────────────────┘          │
│                   Shared Domain Models                      │
└─────────────────────────────────────────────────────────────┘
```

## 🎯 Target Hardware

**Development Board**: ESP32-C6 DevKit (16M Flash variant)

- **Microcontroller**: ESP32-C6 (RISC-V single core @ 160 MHz)
- **Memory**: 16 MB Flash, 512 KB SRAM
- **Wireless**: WiFi 6 (802.11ax), Bluetooth 5.3, Zigbee, Thread
- **GPIO**: 30 programmable GPIOs

### Pin Mapping

| GPIO | Function | Description |
|------|----------|-------------|
| GPIO8 | RGB LED | Smart LED output (via RMT peripheral) |

## 🚀 Quick Start

### Prerequisites

- Rust 1.88+ with RISC-V target:

  ```bash
  rustup target add riscv32imac-unknown-none-elf
  ```

- probe-rs for debugging and flashing
- ESP32-C6 DevKit hardware

### Build & Run

```bash
# Build the firmware
cargo build --release

# Flash and run via probe-rs
cargo run --release

# View defmt logs (if using RTT)
probe-rs rtt --chip esp32c6
```

### Configuration

Default logging level is set to `info` via `DEFMT_LOG` environment variable. Modify `.cargo/config.toml` to adjust.

## 🏛️ Architecture

### Core Technologies

- **Async Runtime**: Embassy executor with timer support
- **Hardware Abstraction**: esp-hal (no_std HAL for ESP32)
- **Memory Management**: esp-alloc with reclaimed RAM
- **Debugging**: probe-rs with RTT logging (defmt)
- **LED Control**: Smart LEDs via RMT peripheral

### Current Functionality

- **RGB LED Demo**: HSV color cycling with brightness control
- **Async Tasks**: Embassy-based concurrent execution
- **RTT Logging**: Real-time debug output via defmt
- **Memory Management**: Dynamic allocation with esp-alloc

### Disabled Features (Ready for Activation)

The codebase includes commented infrastructure for:

- **BLE Stack**: trouble-host with esp-radio integration
- **WiFi Connectivity**: esp-radio framework ready for implementation

## 📁 Project Structure

```plain
sensor-embedded-rs/
├── .agents/
│   └── skills/                 # Agent skills for AI coding assistants
├── .cargo/
│   └── config.toml             # probe-rs runner configuration
├── .clippy.toml                # Clippy linting rules
├── src/
│   ├── bin/
│   │   └── main.rs             # Entry point, HAL init, task spawning
│   ├── lib.rs                  # Library root
│   ├── app.rs                  # App state, commands, channels
│   └── tasks/                  # Async tasks (led, wifi, ble, app)
├── tests/
│   └── hello_test.rs           # Embedded test suite
├── build.rs                    # Build script with linker configuration
├── Cargo.toml                  # Dependencies and features
└── rust-toolchain.toml         # Required Rust toolchain
```

### Key Files

- `src/bin/main.rs`: Entry point with HAL init and task spawning
- `src/app.rs`: App state, commands, and inter-task channels
- `src/tasks/`: Embassy async tasks (led, wifi, ble, app)
- `Cargo.toml`: Dependencies and features for ESP32-C6, BLE, WiFi
- `.cargo/config.toml`: probe-rs runner and build configuration
- `build.rs`: Linker script management and error handling
- `.clippy.toml`: Clippy linting rules

## 🔧 Development

### Build Profiles

- **Development**: Optimized for size (`opt-level = "s"`)
- **Release**: Full optimizations with LTO (`lto = 'fat'`)

### Testing

```bash
# Run embedded tests
cargo test

# Test with specific output
cargo test -- --nocapture
```

### Linting

```bash
# Check code style
cargo clippy

# Format code
cargo fmt
```

## 🔮 Future Plans

Future releases will expand capabilities to support:

- **BLE Connectivity**: Sensor data broadcasting and mesh networking
- **WiFi Integration**: HTTP-based communication with sensor service
- **Sensor Support**: Integration with various sensor types
- **Power Management**: Deep sleep and battery optimization
- **Over-the-Air Updates**: Secure firmware update mechanism

## 📚 References

### Essential Documentation

- **[Rust on ESP Book](https://docs.esp-rs.org/book/)** - Comprehensive ESP32 development guide
- **[Embassy](https://embassy.dev/)** - Async framework for embedded systems
- **[probe-rs](https://probe.rs/)** - Debugging and flashing tooling
- **[esp-hal](https://github.com/esp-rs/esp-hal)** - Hardware abstraction layer
- **[defmt](https://defmt.ferrous-systems.com/)** - Efficient logging framework

### Related Projects

- **[trouble-host](https://github.com/embassy-rs/trouble)** - BLE stack for embedded devices
- **[esp-radio](https://github.com/esp-rs/esp-radio)** - Wireless communication abstraction

---

**Built with ❤️ and Rust for Embedded Systems** 🦀

Connected to the broader Sensor Platform ecosystem for edge-to-cloud sensor data management.
