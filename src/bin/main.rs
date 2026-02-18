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
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use esp_alloc as _;
use esp_hal::gpio::AnyPin;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{Config, clock::CpuClock, rmt::Rmt, time::Rate};

use esp_hal_smartled::{SmartLedsAdapterAsync, buffer_size_async};
use smart_leds::{
    RGB8, SmartLedsWriteAsync, brightness, gamma,
    hsv::{Hsv, hsv2rgb},
};

use esp_radio::ble::controller::BleConnector;
use trouble_host::prelude::*;
// use bt_hci::controller::ExternalController;

extern crate alloc;

// Constants
const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 1;

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

// More information: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>

// More esp-hal examples: https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples
// More esp-hal / ble examples: https://github.com/esp-rs/esp-hal/blob/1.0.0/examples/ble/bas_peripheral/src/main.rs

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.2.0
    rtt_target::rtt_init_defmt!();

    let config = Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    // Initialize radio stack
    let radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");

    // find more examples https://github.com/embassy-rs/trouble/tree/main/examples/esp32
    let transport = BleConnector::new(&radio_init, peripherals.BT, Default::default()).unwrap();
    let ble_controller = ExternalController::<_, 1>::new(transport);

    let address: Address = Address::random([0xff, 0x8f, 0x1a, 0x05, 0xe4, 0xff]);
    info!("Our address = {:?}", address);

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let stack = trouble_host::new(ble_controller, &mut resources).set_random_address(address);

    let Host {
        mut peripheral,
        runner,
        ..
    } = stack.build();

    // Initialize RMT and SmartLed
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80))
        .expect("Failed to initialize RMT")
        .into_async();

    // Use `spawner` to launch tasks.
    spawner.spawn(main_task()).ok();
    spawner.spawn(led_task(rmt, peripherals.GPIO8.into())).ok();

    loop {
        info!("Tick!");
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn main_task() {
    loop {
        info!("Task!");
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn led_task(rmt: Rmt<'static, esp_hal::Async>, pin: AnyPin<'static>) {
    let mut rmt_buffer = [esp_hal::rmt::PulseCode::default(); buffer_size_async(1)];
    let mut led = SmartLedsAdapterAsync::new(rmt.channel0, pin, &mut rmt_buffer);

    let level = 10;
    let mut color = Hsv {
        hue: 0,
        sat: 255,
        val: 255,
    };
    let mut data: RGB8 = hsv2rgb(color);

    // led init sequence
    for n in 0..10 {
        let init_level = if (n % 2) == 0 { level } else { 0 };
        _ = led
            .write(brightness(gamma([data].into_iter()), init_level))
            .await;
        Timer::after_millis(200).await;
    }

    // led main loop
    loop {
        for hue in 0..=255 {
            color.hue = hue;

            // Convert from the HSV color space (where we can easily transition from one
            // color to the other) to the RGB color space that we can then send to the LED
            data = hsv2rgb(color);

            // When sending to the LED, we do a gamma correction first (see smart_leds
            // documentation for details) and then limit the brightness to 10 out of 255 so
            // that the output is not too bright.
            _ = led
                .write(brightness(gamma([data].into_iter()), level))
                .await;

            Timer::after_millis(50).await;
        }

        Timer::after_millis(200).await;
    }
}
