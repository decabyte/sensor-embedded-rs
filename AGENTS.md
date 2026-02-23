# AGENTS.md - Agent Guidelines for sensor-embedded-rs

This file provides guidelines for agentic coding agents working on this ESP32-C6 embedded Rust project.

---

## Build Commands

```bash
# Build release firmware (recommended for device)
cargo build --release

# Build debug (slower on device, useful for debugging)
cargo build

# Flash and run via probe-rs (auto-flashes via JTAG)
cargo run --release

# View RTT logs (separate terminal)
probe-rs rtt --chip esp32c6
```

## Lint & Format

```bash
# Run clippy lints (warnings become errors)
cargo clippy -- -D warnings

# Format code
cargo fmt --check   # Check only
cargo fmt           # Format
```

## Testing

Tests are **disabled by default** in Cargo.toml to avoid running on host. To run:

```bash
# Run all tests (if enabled)
cargo test

# Run a single test by name
cargo test --test hello_test hello_test

# Run with output
cargo test -- --nocapture
```

**Note**: Tests require embedded-test.x linker script and `test = true` in Cargo.toml.

---

## Code Style Guidelines

### General

- **Edition**: 2024 (latest)
- **Rust version**: 1.92+ (see rust-toolchain.toml)
- **Target**: riscv32imac-unknown-none-elf
- **No std**: All code must be `#![no_std]` compatible
- **Clippy**: Warnings are errors (`-D warnings`)

### Required Attributes

```rust
#![no_std]
#![no_main]

// At top of main.rs:
#![deny(clippy::large_stack_frames)]
#![deny(clippy::mem_forget)]
```

### Imports

Group imports by crate (external first, then local):

```rust
use defmt::info;
use embassy_executor::Spawner;
use embassy_time::Timer;

use esp_hal::{clock::CpuClock, rmt::Rmt, time::Rate};
use esp_alloc as _;

use sensor_embedded_rs::tasks::{led_task, wifi_task};
```

- Use absolute paths for esp-hal types
- Use `use crate::` for local modules
- Avoid wildcard imports

### Naming Conventions

- **Snake_case** for variables, functions, modules: `led_task`, `wifi_controller`
- **PascalCase** for types, traits, enums: `AppMode`, `BleConnector`
- **SCREAMING_SNAKE_CASE** for consts: `MAX_CONNECTIONS`
- **CamelCase** for struct fields only when idiomatic

### Async & Tasks

Use Embassy task attribute:

```rust
#[embassy_executor::task]
pub async fn my_task(/* params */) {
    // Task implementation
}
```

Spawn tasks with `must_spawn`:

```rust
spawner.must_spawn(led_task(rmt, pin));
spawner.must_spawn(wifi_task(spawner, controller, interface));
```

### Memory & Static Data

Prefer **heapless** collections with compile-time capacity:

```rust
use heapless::String;
use heapless::Vec;

let buf: String<64> = String::new();
let mut data: Vec<u8, 256> = Vec::new();
```

Use **StaticCell** for static lifetime:

```static}static MY_DATA: StaticCell<Channel<...>> = StaticCell::new();

let channel = MY_DATA.init(Channel::new());
```

### Logging

Use **defmt** for efficient embedded logging:

```rust
use defmt::info;
use defmt::warn;
use defmt::error;

info!("Task started with value: {}", value);
```

For custom types, derive `defmt::Format`:

```rust
#[derive(defmt::Format)]
pub struct MyStruct {
    pub field: u32,
}
```

### Error Handling

Prefer Result types with custom error enums:

```rust
#[derive(Debug, defmt::Format)]
pub enum Error {
    InitFailed,
    Network(NetworkError),
    Timeout,
}

impl From<NetworkError> for Error {
    fn from(e: NetworkError) -> Self {
        Error::Network(e)
    }
}
```

In async code, propagate with `?`:

```rust
let config = wifi::new(radio_init, peripherals.WIFI, Default::default())
    .expect("Failed to initialize Wi-Fi");
```

### Hardware Patterns

HAL initialization pattern:

```rust
let config = Config::default().with_cpu_clock(CpuClock::max());
let peripherals = esp_hal::init(config);
```

Peripheral into async:

```rust
let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80))
    .expect("Failed to initialize RMT")
    .into_async();
```

### Inter-Task Communication

Use **embassy_sync** primitives:

- **Channel** for multiple values: `Channel<CriticalSectionRawMutex, AppCommand, 4>`
- **Watch** for state broadcasts: `Watch<CriticalSectionRawMutex, AppState, 4>`
- **Signal** for single notifications: `Signal<CriticalSectionRawMutex, ()>`

```rust
// Sender
CMD_CHANNEL.send(command).await;

// Receiver
let cmd = CMD_CHANNEL.receive().await;
```

### Heap Allocation

Use **esp-alloc** for dynamic allocation:

```rust
esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);
```

### Clippy Configuration

Project uses `.clippy.toml` with custom settings:

```toml
stack-size-threshold = 1024
```

When large stack frames are unavoidable, allow them:

```rust
#[allow(
    clippy::large_stack_frames,
    reason = "main allocates large buffers for initialization"
)]
```

---

## Project Structure

```
sensor-embedded-rs/
├── src/
│   ├── bin/main.rs      # Entry point, HAL init, task spawning
│   ├── lib.rs           # Library root
│   ├── app.rs           # App state, commands, channels
│   └── tasks/           # Async tasks (led, wifi, ble, app)
├── .cargo/config.toml   # probe-rs runner, build config
├── .clippy.toml         # Clippy settings
├── build.rs             # Linker script handling
└── Cargo.toml           # Dependencies, features
```

---

## Common Build Errors

| Error | Solution |
|-------|----------|
| `undefined symbol: _stack_start` | Check linkall.x in build.rs |
| `undefined symbol: esp_rtos_initialized` | Call `esp_rtos::start()` |
| `defmt not found` | Add `use defmt_rtt as _;` |
| `mem::forget` warnings | Avoid mem::forget with HAL types |

---

## Dependency Notes

- **esp-hal**: Pins/GPIO use new API with `OutputConfig`
- **Embassy crates**: Update together (executor, time, sync, net)
- **esp-radio**: Requires esp-rtos scheduler initialization

---

## References

- [Rust on ESP Book](https://docs.esp-rs.org/book/)
- [Embassy Docs](https://embassy.dev/)
- [esp-hal GitHub](https://github.com/esp-rs/esp-hal)
- [defmt Docs](https://defmt.ferrous-systems.com/)
