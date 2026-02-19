#![no_std]
#![no_main]
#![deny(clippy::large_stack_frames)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use panic_rtt_target as _;

use defmt::info;
use static_cell::StaticCell;

use embassy_executor::Spawner;

use esp_alloc as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{Config, clock::CpuClock, rmt::Rmt, time::Rate};

use esp_radio::ble::controller::BleConnector;
use esp_radio::wifi;

use sensor_embedded_rs::tasks::{
    app_task::app_task, ble_task::ble_task, led_task::led_task, wifi_task::wifi_task,
};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

static RADIO_INIT: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    // Logging
    rtt_target::rtt_init_defmt!();

    // HAL initialization
    let config = Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Heap initialization
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);
    // COEX needs more RAM
    esp_alloc::heap_allocator!(size: 64 * 1024);

    // Embassy initialization
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    // Initialize RMT
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80))
        .expect("Failed to initialize RMT")
        .into_async();

    // Initialize Radio Stack
    let radio_init =
        RADIO_INIT.init(esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller"));

    // Initialize BLE
    let ble_connector = BleConnector::new(radio_init, peripherals.BT, Default::default()).unwrap();

    // Initialize WiFi
    let (wifi_controller, interfaces) = wifi::new(radio_init, peripherals.WIFI, Default::default())
        .expect("Failed to initialize Wi-Fi");

    // Use `spawner` to launch tasks
    spawner.must_spawn(app_task());
    spawner.must_spawn(led_task(rmt, peripherals.GPIO8.into()));
    spawner.must_spawn(ble_task(spawner, ble_connector));
    spawner.must_spawn(wifi_task(spawner, wifi_controller, interfaces.sta));

    info!("All tasks spawned");
}

// References:
// - app_image_format: https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description
// - esp-hal examples: https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples
// - esp-hal ble examples: https://github.com/esp-rs/esp-hal/blob/1.0.0/examples/ble/bas_peripheral/src/main.rs
// - trouble examples: https://github.com/embassy-rs/trouble/tree/main/examples/esp32
