use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::watch::Watch;

#[derive(Clone, Copy, PartialEq, defmt::Format)]
pub enum AppMode {
    Idle,
    Advertising,
    Infrastructure,
}

impl AppMode {
    pub fn to_byte(&self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Advertising => 1,
            Self::Infrastructure => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, defmt::Format)]
pub enum WifiState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Clone, Copy, defmt::Format)]
pub struct AppConfig {
    pub wifi_ssid: [u8; 32],
    pub wifi_pass: [u8; 64],
}

impl AppConfig {
    pub const fn default() -> Self {
        Self {
            wifi_ssid: [0u8; 32],
            wifi_pass: [0u8; 64],
        }
    }

    pub fn ssid_str(&self) -> &str {
        let len = self.wifi_ssid.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.wifi_ssid[..len]).unwrap_or("")
    }

    pub fn pass_str(&self) -> &str {
        let len = self.wifi_pass.iter().position(|&b| b == 0).unwrap_or(64);
        core::str::from_utf8(&self.wifi_pass[..len]).unwrap_or("")
    }
}

#[derive(Clone, Copy, defmt::Format)]
pub struct AppState {
    pub mode: AppMode,
    pub config: AppConfig,
    pub wifi_state: WifiState,
}

impl AppState {
    pub const fn default() -> Self {
        Self {
            mode: AppMode::Idle,
            config: AppConfig::default(),
            wifi_state: WifiState::Disconnected,
        }
    }
}

/// Commands sent TO app_task from other tasks (BLE writes, wifi status, etc.)
#[derive(defmt::Format)]
pub enum AppCommand {
    SetMode(AppMode),
    UpdateConfig(AppConfig),
    UpdateWifiState(WifiState),
}

// app_task reads commands from this; senders never need to queue more than 1-2 at a time
pub static CMD_CHANNEL: Channel<CriticalSectionRawMutex, AppCommand, 4> = Channel::new();

// app_task broadcasts state; up to 4 simultaneous receivers (ble, wifi, led, power)
pub static STATE_WATCH: Watch<CriticalSectionRawMutex, AppState, 4> = Watch::new();
