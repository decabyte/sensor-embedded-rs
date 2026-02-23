use defmt::{error, info, warn};
use static_cell::StaticCell;

use embassy_executor::Spawner;
use embassy_net::{Config, Runner, StackResources};
use embassy_time::Timer;
use esp_radio::wifi::{ClientConfig, ModeConfig, WifiController, WifiDevice};

extern crate alloc;

use crate::app::{AppCommand, AppMode, CMD_CHANNEL, STATE_WATCH, WifiState};

const NET_SOCKETS: usize = 4;

static NET_RESOURCES: StaticCell<StackResources<NET_SOCKETS>> = StaticCell::new();

/// Drives the embassy-net stack continuously. Spawned once by wifi_task.
#[embassy_executor::task]
async fn net_runner_task(mut runner: Runner<'static, WifiDevice<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
pub async fn wifi_task(
    spawner: Spawner,
    mut controller: WifiController<'static>,
    device: WifiDevice<'static>,
) -> ! {
    let resources = NET_RESOURCES.init(StackResources::new());
    // Fixed seed — acceptable for embedded; replace with hardware RNG if available.
    let seed: u64 = 0x_dead_beef_cafe_1234;
    let config = Config::dhcpv4(Default::default());
    let (stack, runner) = embassy_net::new(device, config, resources, seed);

    spawner.must_spawn(net_runner_task(runner));

    let mut rx = STATE_WATCH
        .receiver()
        .expect("wifi_task: no Watch receiver slot");

    info!("[wifi] started");

    loop {
        // Wait until Infrastructure mode is requested ───────────────────────────
        loop {
            let state = rx.changed().await;
            if state.mode != AppMode::Infrastructure {
                continue;
            }

            let ssid = state.config.ssid_str();
            let pass = state.config.pass_str();

            if ssid.is_empty() {
                warn!("[wifi] SSID is empty, cannot connect");
                CMD_CHANNEL
                    .send(AppCommand::SetMode(AppMode::Advertising))
                    .await;
                continue;
            }

            CMD_CHANNEL
                .send(AppCommand::UpdateWifiState(WifiState::Connecting))
                .await;

            let client_cfg = ClientConfig::default()
                .with_ssid(alloc::string::String::from(ssid))
                .with_password(alloc::string::String::from(pass));

            if let Err(e) = controller.set_config(&ModeConfig::Client(client_cfg)) {
                error!("[wifi] set_config: {:?}", e);
                CMD_CHANNEL
                    .send(AppCommand::UpdateWifiState(WifiState::Error))
                    .await;
                CMD_CHANNEL
                    .send(AppCommand::SetMode(AppMode::Advertising))
                    .await;
                continue;
            }

            if let Err(e) = controller.start_async().await {
                error!("[wifi] start: {:?}", e);
                CMD_CHANNEL
                    .send(AppCommand::UpdateWifiState(WifiState::Error))
                    .await;
                CMD_CHANNEL
                    .send(AppCommand::SetMode(AppMode::Advertising))
                    .await;
                continue;
            }

            info!("[wifi] connecting to '{}'", ssid);
            if let Err(e) = controller.connect_async().await {
                error!("[wifi] connect: {:?}", e);
                let _ = controller.stop_async().await;
                CMD_CHANNEL
                    .send(AppCommand::UpdateWifiState(WifiState::Error))
                    .await;
                CMD_CHANNEL
                    .send(AppCommand::SetMode(AppMode::Advertising))
                    .await;
                continue;
            }

            info!("[wifi] associated, waiting for DHCP lease");

            while !stack.is_link_up() {
                Timer::after_millis(500).await;
            }

            while stack.config_v4().is_none() {
                Timer::after_millis(500).await;
            }

            if let Some(config) = stack.config_v4() {
                info!("[wifi] IP: {}", config.address);
                if let Some(gateway) = config.gateway {
                    info!("[wifi] gateway: {}", gateway);
                }
                for dns in &config.dns_servers {
                    info!("[wifi] DNS: {}", dns);
                }
            }

            CMD_CHANNEL
                .send(AppCommand::UpdateWifiState(WifiState::Connected))
                .await;

            break;
        }

        // Wait until mode leaves Infrastructure, then disconnect ──────────────────
        loop {
            let state = rx.changed().await;
            if state.mode != AppMode::Infrastructure {
                info!("[wifi] mode={}, disconnecting", state.mode);
                let _ = controller.disconnect_async().await;
                let _ = controller.stop_async().await;
                CMD_CHANNEL
                    .send(AppCommand::UpdateWifiState(WifiState::Disconnected))
                    .await;
                break;
            }
        }
    }
}
