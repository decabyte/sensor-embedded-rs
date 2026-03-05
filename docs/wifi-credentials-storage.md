# WiFi Credentials Storage with esp-storage

## Overview

Add persistent flash storage for WiFi credentials using `esp-storage`,
initialized in `main.rs` and consumed by `app_task` via the existing `STATE_WATCH` / `CMD_CHANNEL` infrastructure.

---

## Current Architecture

```text
main.rs                    app_task.rs              wifi_task.rs
    │                           │                         │
    ├─ Init HAL                 │                         │
    ├─ Init WiFi/BLE           │                         │
    ├─ Spawn app_task() ───────►│                         │
    ├─ Spawn wifi_task() ──────►│◄── STATE_WATCH ───────►│
    └─ Spawn ble_task() ───────►│◄── CMD_CHANNEL ───────►│
```

- `AppConfig` (in `app.rs`): Contains `wifi_ssid: [u8; 32]` and `wifi_pass: [u8; 64]`
- `app_task` manages state and broadcasts via `STATE_WATCH`
- Credentials currently come from BLE writes → `AppCommand::UpdateConfig`

---

## Implementation Steps

### Step 1: Add Dependency

**File:** `Cargo.toml`

```toml
esp-storage = "0.4.0"
```

---

### Step 2: Create Storage Module

**New file:** `src/storage.rs`

```rust
#![no_std]

use esp_storage::FlashStorage;
use sensor_embedded_rs::app::AppConfig;

#[derive(Debug, defmt::Format)]
pub enum Error {
    Read,
    Write,
    InvalidData,
}

pub struct CredentialsStorage {
    flash: FlashStorage,
}

impl CredentialsStorage {
    pub fn new() -> Self {
        Self {
            flash: FlashStorage::new(),
        }
    }

    pub fn load(&self) -> Result<AppConfig, Error> {
        let mut buf = [0u8; core::mem::size_of::<AppConfig>()];
        self.flash.read(0, &mut buf).map_err(|_| Error::Read)?;

        // Validate magic or check for empty
        let config = core::ptr::read_unaligned(buf.as_ptr() as *const AppConfig);
        if config.wifi_ssid.iter().all(|&b| b == 0) {
            return Ok(AppConfig::default());
        }
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> Result<(), Error> {
        let buf = unsafe {
            core::slice::from_raw_parts(
                config as *const AppConfig as *const u8,
                core::mem::size_of::<AppConfig>(),
            )
        };
        self.flash.write(0, buf).map_err(|_| Error::Write)
    }
}
```

**Notes:**

- Uses offset 0 within flash storage (partition-dependent)
- Currently reads/writes raw `AppConfig` bytes — may want a magic/version header
- Returns `AppConfig::default()` if flash is empty (all zeros)

---

### Step 3: Initialize Storage in main.rs

**File:** `src/bin/main.rs`

```rust
use sensor_embedded_rs::app::AppConfig;
use sensor_embedded_rs::storage::CredentialsStorage;
use static_cell::StaticCell;

// Store initial config for app_task to read
static INITIAL_CONFIG: StaticCell<AppConfig> = StaticCell::new();

// In main(), after heap initialization:
let storage = CredentialsStorage::new();
let config = storage.load().unwrap_or_default();
let saved_config = INITIAL_CONFIG.init(config);

// Pass saved_config to app_task via a new command or static
spawner.must_spawn(app_task(saved_config));
```

---

### Step 4: Update app_task to Accept Initial Config

**File:** `src/tasks/app_task.rs`

```rust
#[embassy_executor::task]
pub async fn app_task(initial_config: AppConfig) -> ! {
    let sender = STATE_WATCH.sender();

    let mut state = AppState::default();
    state.config = initial_config;  // Use stored credentials
    state.mode = AppMode::Advertising;
    sender.send(state);

    // ... rest of existing logic
}
```

---

### Step 5: Save Credentials on Update

**File:** `src/tasks/app_task.rs`

Add storage saving when credentials are updated (after debounce):

```rust
use crate::storage::CredentialsStorage;
use static_cell::StaticCell;

static CREDENTIALS_STORAGE: StaticCell<CredentialsStorage> = StaticCell::new();

// In app_task initialization:
let storage = CREDENTIALS_STORAGE.init(CredentialsStorage::new());

// In UpdateConfig handler, after debounce:
if !state.config.ssid_str().is_empty() {
    if let Err(e) = storage.save(&state.config) {
        warn!("[app] failed to save credentials: {:?}", e);
    }
}
```

---

## Partition Table Requirements

You will need to allocate flash space for credentials. Options:

### Option A: Use unpartitioned flash

`esp-storage` can use raw flash sectors directly (check ESP32-C6 available sectors).

### Option B: Add partition entry

In your partition CSV (e.g., `partitions.csv`):

```text
credentials, data, spiffs, 0x10000, 0x1000,
```

Then configure `esp-storage` to use that partition via `FlashStorage::newpartition()`.

---

## Open Questions

1. **Flash offset**: What partition/offset should credentials use?
2. **Fallback**: Should there be default credentials if flash is empty, or start with empty (no auto-connect)?
3. **BLE read**: Should BLE be able to read current credentials to display in a config app?
4. **Versioning**: Should we add a magic number/version header to handle schema migrations?

---

## References

- [esp-storage crate](https://crates.io/crates/esp-storage)
- [ESP32-C6 Flash documentation](https://docs.espressif.com/projects/esp-idf/en/latest/esp32c6/api-reference/storage/spi_flash.html)
- Partition table: <https://docs.espressif.com/projects/esp-idf/en/latest/esp32c6/api-guides/partition_table.html>
