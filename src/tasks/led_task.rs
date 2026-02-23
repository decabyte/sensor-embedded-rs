use defmt::info;

use embassy_time::Timer;

use esp_hal::gpio::AnyPin;
use esp_hal::rmt::Rmt;

use esp_hal_smartled::{SmartLedsAdapterAsync, buffer_size_async};
use smart_leds::{
    RGB8, SmartLedsWriteAsync, brightness, gamma,
    hsv::{Hsv, hsv2rgb},
};

use crate::app::{AppMode, STATE_WATCH};

const HUE_INIT: u8 = 43; // Yellow
const HUE_IDLE: u8 = 213; // Pink
const HUE_ADVERTISING: u8 = 171; // Blue
const HUE_INFRASTRUCTURE: u8 = 85; // Green

#[embassy_executor::task]
pub async fn led_task(rmt: Rmt<'static, esp_hal::Async>, pin: AnyPin<'static>) {
    let mut rmt_buffer = [esp_hal::rmt::PulseCode::default(); buffer_size_async(1)];
    let mut led = SmartLedsAdapterAsync::new(rmt.channel0, pin, &mut rmt_buffer);

    let level = 10;
    let mut color = Hsv {
        hue: HUE_INIT,
        sat: 255,
        val: 255,
    };
    let mut data: RGB8 = hsv2rgb(color);

    info!("[led] started");

    // led init sequence
    for n in 0..10 {
        let init_level = if (n % 2) == 0 { level } else { 0 };
        _ = led
            .write(brightness(gamma([data].into_iter()), init_level))
            .await;
        Timer::after_millis(200).await;
    }

    // fetch app state
    let mut rx = STATE_WATCH
        .receiver()
        .expect("led_task: no Watch receiver slot");

    // led main loop
    loop {
        let state = rx.try_get();

        // Set color based on AppMode
        let mode = state.map(|s| s.mode).unwrap_or(AppMode::Advertising);
        color.hue = match mode {
            AppMode::Idle => HUE_IDLE,
            AppMode::Advertising => HUE_ADVERTISING,
            AppMode::Infrastructure => HUE_INFRASTRUCTURE,
        };

        // Convert from the HSV color space (where we can easily transition from one
        // color to the other) to the RGB color space that we can then send to the LED
        data = hsv2rgb(color);

        for level in 0..=15 {
            // When sending to the LED, we do a gamma correction first (see smart_leds
            // documentation for details) and then limit the brightness to 10 out of 255 so
            // that the output is not too bright.
            _ = led
                .write(brightness(gamma([data].into_iter()), level))
                .await;

            Timer::after_millis(100).await;
        }

        for level in (0..=15).rev() {
            _ = led
                .write(brightness(gamma([data].into_iter()), level))
                .await;

            Timer::after_millis(100).await;
        }

        Timer::after_millis(400).await;
    }
}
