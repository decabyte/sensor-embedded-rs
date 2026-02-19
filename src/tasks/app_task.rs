use defmt::{info, warn};
use embassy_executor::task;

use crate::app::{AppCommand, AppMode, AppState, CMD_CHANNEL, STATE_WATCH};

#[task]
pub async fn app_task() -> ! {
    let sender = STATE_WATCH.sender();

    // Publish initial state so all receivers can get their first value
    let mut state = AppState::default();
    state.mode = AppMode::Local;
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

                // Validate transition
                let valid = matches!(
                    (state.mode, new_mode),
                    (AppMode::Idle, AppMode::Local)
                        | (AppMode::Local, AppMode::Idle)
                        | (AppMode::Local, AppMode::Connected)
                        | (AppMode::Connected, AppMode::Local)
                        | (AppMode::Connected, AppMode::Idle)
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
                info!("[app] config updated");
                state.config = new_config;
                sender.send(state);
            }
        }
    }
}
