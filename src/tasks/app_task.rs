use defmt::{info, warn};
use embassy_time::{Duration, Timer};

use crate::app::{AppCommand, AppMode, AppState, CMD_CHANNEL, STATE_WATCH, WifiState};

const CONFIG_DEBOUNCE_MS: u64 = 500;

#[embassy_executor::task]
pub async fn app_task() -> ! {
    let sender = STATE_WATCH.sender();

    let mut state = AppState::default();
    state.mode = AppMode::Advertising;
    sender.send(state);

    info!("[app] started, mode = {}", state.mode);

    loop {
        let cmd = CMD_CHANNEL.receive().await;
        match cmd {
            AppCommand::SetMode(new_mode) => {
                if new_mode == state.mode {
                    continue;
                }
                info!("[app] mode {} -> {}", state.mode, new_mode);

                let valid = matches!(
                    (state.mode, new_mode),
                    (AppMode::Idle, AppMode::Advertising)
                        | (AppMode::Advertising, AppMode::Idle)
                        | (AppMode::Advertising, AppMode::Infrastructure)
                        | (AppMode::Infrastructure, AppMode::Advertising)
                        | (AppMode::Infrastructure, AppMode::Idle)
                );

                if valid {
                    state.mode = new_mode;
                    sender.send(state);
                } else {
                    warn!(
                        "[app] invalid mode transition {} -> {}",
                        state.mode, new_mode
                    );
                }
            }
            AppCommand::UpdateConfig(new_config) => {
                info!("[app] config updated, debouncing {}ms", CONFIG_DEBOUNCE_MS);
                state.config = new_config;
                sender.send(state);

                Timer::after(Duration::from_millis(CONFIG_DEBOUNCE_MS)).await;

                let ssid = state.config.ssid_str();
                if !ssid.is_empty() && state.mode == AppMode::Advertising {
                    info!(
                        "[app] valid credentials after debounce, transitioning to Infrastructure"
                    );
                    state.mode = AppMode::Infrastructure;
                    sender.send(state);
                }
            }
            AppCommand::UpdateWifiState(new_wifi_state) => {
                if new_wifi_state != state.wifi_state {
                    info!(
                        "[app] wifi_state {} -> {}",
                        state.wifi_state, new_wifi_state
                    );
                    state.wifi_state = new_wifi_state;
                    sender.send(state);

                    // Transition to Advertising if WiFi fails while in Infrastructure
                    if (new_wifi_state == WifiState::Error
                        || new_wifi_state == WifiState::Disconnected)
                        && state.mode == AppMode::Infrastructure
                    {
                        info!("[app] wifi error/disconnected, transitioning to Advertising");
                        state.mode = AppMode::Advertising;
                        sender.send(state);
                    }
                }
            }
        }
    }
}
