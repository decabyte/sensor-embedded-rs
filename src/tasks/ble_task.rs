use defmt::{info, warn};
use static_cell::StaticCell;

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use esp_radio::ble::controller::BleConnector;

use trouble_host::prelude::*;
// use bt_hci::controller::ExternalController;

use crate::app::{AppCommand, AppConfig, AppMode, CMD_CHANNEL, STATE_WATCH};

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2;

// ── GATT Server definition ─────────────────────────────────────────────────
//
// CriticalSectionRawMutex is required for no_std targets where NoopRawMutex
// may not satisfy the RawMutex bound used by the gatt_server expansion.

#[gatt_server(connections_max = CONNECTIONS_MAX, mutex_type = CriticalSectionRawMutex, attribute_table_size = 24)]
struct BleServer {
    device_info: DeviceInfoService,
    config_svc: ConfigService,
    status_svc: StatusService,
}

/// Standard Device Information Service (0x180A).
#[gatt_service(uuid = service::DEVICE_INFORMATION)]
struct DeviceInfoService {
    #[characteristic(uuid = characteristic::FIRMWARE_REVISION_STRING, read, value = *b"0.1.0")]
    firmware_rev: [u8; 5],
}

/// Custom Config Service — BLE central writes WiFi credentials here.
#[gatt_service(uuid = "4368616e-6e65-6c73-436f-6e666967001a")]
struct ConfigService {
    #[characteristic(uuid = "4368616e-6e65-6c73-436f-6e6669670001", write)]
    ssid: heapless::Vec<u8, 32>,
    #[characteristic(uuid = "4368616e-6e65-6c73-436f-6e6669670002", write)]
    password: heapless::Vec<u8, 64>,
}

/// Custom Status Service — notifies current mode as a single byte.
#[gatt_service(uuid = "4368616e-6e65-6c73-5374-617475730001")]
struct StatusService {
    /// AppMode: 0=Idle, 1=Local, 2=Connected
    #[characteristic(
        uuid = "4368616e-6e65-6c73-5374-617475730002",
        read,
        notify,
        value = 0u8
    )]
    mode: u8,
}

static RESOURCES: StaticCell<
    HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX>,
> = StaticCell::new();
static STACK: StaticCell<
    Stack<'static, ExternalController<BleConnector<'static>, 2>, DefaultPacketPool>,
> = StaticCell::new();
static RUNNER: StaticCell<
    Runner<'static, ExternalController<BleConnector<'static>, 2>, DefaultPacketPool>,
> = StaticCell::new();

#[embassy_executor::task]
async fn ble_runner_task(
    runner: &'static mut Runner<
        'static,
        ExternalController<BleConnector<'static>, 2>,
        DefaultPacketPool,
    >,
) {
    let _ = runner.run().await;
}

#[embassy_executor::task]
pub async fn ble_task(spawner: Spawner, connector: BleConnector<'static>) -> ! {
    let ble_controller: ExternalController<_, 2> = ExternalController::new(connector);

    let resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let resources = RESOURCES.init(resources);

    let stack = trouble_host::new(ble_controller, resources)
        .set_random_address(Address::random([0xde, 0xad, 0xbe, 0xef, 0x01, 0x02]));
    let stack = STACK.init(stack);

    let Host {
        mut peripheral,
        runner,
        ..
    } = stack.build();

    let runner = RUNNER.init(runner);

    let gap = GapConfig::Peripheral(PeripheralConfig {
        name: "SensorEmbedded",
        appearance: &appearance::UNKNOWN,
    });
    let server = BleServer::new_with_config(gap).expect("BleServer init failed");

    let mut rx = STATE_WATCH
        .receiver()
        .expect("ble_task: no Watch receiver slot");

    info!("[ble] started");

    spawner.must_spawn(ble_runner_task(runner));

    loop {
        // Only advertise when not Idle
        loop {
            let state = rx.changed().await;
            if state.mode != AppMode::Idle {
                break;
            }
            info!("[ble] idle — not advertising");
        }

        info!("[ble] starting advertisement");

        let mut adv_data = [0u8; 31];
        let adv_len = AdStructure::encode_slice(
            &[
                AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
                AdStructure::CompleteLocalName(b"SensorEmbedded"),
            ],
            &mut adv_data,
        )
        .unwrap_or(0);

        let acceptor = match peripheral
            .advertise(
                &Default::default(),
                Advertisement::ConnectableScannableUndirected {
                    adv_data: &adv_data[..adv_len],
                    scan_data: &[],
                },
            )
            .await
        {
            Ok(a) => a,
            Err(_) => {
                warn!("[ble] advertise error");
                continue;
            }
        };

        let conn = match acceptor.accept().await {
            Ok(c) => c,
            Err(_) => {
                warn!("[ble] accept error");
                continue;
            }
        };

        let conn = match conn.with_attribute_server(&server) {
            Ok(c) => c,
            Err(_) => {
                warn!("[ble] with_attribute_server error");
                continue;
            }
        };

        info!("[ble] connected");

        // Notify current mode immediately
        if let Some(state) = rx.try_changed() {
            let byte = state.mode.to_byte();
            let _ = server.status_svc.mode.notify(&conn, &byte).await;
        }

        let mut ssid_buf = [0u8; 32];
        let mut pass_buf = [0u8; 64];
        let mut ssid_len = 0usize;

        loop {
            match conn.next().await {
                GattConnectionEvent::Disconnected { reason: _ } => {
                    info!("[ble] disconnected");
                    break;
                }
                GattConnectionEvent::Gatt {
                    event: GattEvent::Write(event),
                } => {
                    let handle = event.handle();
                    event.accept().unwrap().send().await;

                    if handle == server.config_svc.ssid.handle {
                        let val: heapless::Vec<u8, 32> = server
                            .table()
                            .get(&server.config_svc.ssid)
                            .unwrap_or_default();
                        ssid_len = val.len().min(32);
                        ssid_buf[..ssid_len].copy_from_slice(&val[..ssid_len]);
                        info!("[ble] SSID written ({} bytes)", ssid_len);
                    } else if handle == server.config_svc.password.handle {
                        let val: heapless::Vec<u8, 64> = server
                            .table()
                            .get(&server.config_svc.password)
                            .unwrap_or_default();
                        let pass_len = val.len().min(64);
                        pass_buf[..pass_len].copy_from_slice(&val[..pass_len]);
                        info!("[ble] password written ({} bytes)", pass_len);

                        // Both credentials received — push config + request Connected
                        let mut config = AppConfig::default();
                        config.wifi_ssid[..ssid_len].copy_from_slice(&ssid_buf[..ssid_len]);
                        config.wifi_pass[..pass_len].copy_from_slice(&pass_buf[..pass_len]);
                        CMD_CHANNEL.send(AppCommand::UpdateConfig(config)).await;
                        CMD_CHANNEL
                            .send(AppCommand::SetMode(AppMode::Connected))
                            .await;
                    }
                }
                _ => {}
            }

            // Send status notification on mode change
            if let Some(state) = rx.try_changed() {
                let byte = state.mode.to_byte();
                let _ = server.status_svc.mode.notify(&conn, &byte).await;

                if state.mode == AppMode::Idle {
                    break;
                }
            }
        }
    }
}
