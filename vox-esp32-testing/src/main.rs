#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]
extern crate alloc;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use vox_esp32_core::esp32_led::{LEDColor, LEDStatus};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let mut controller =
        vox_esp32_core::init(spawner).expect("failed to initialize ESP controller");

    controller
        .led
        .set(Some(LEDColor::Purple), Some(LEDStatus::Blink), None, None)
        .await;

    let mut col_i = 0;
    let colors: [LEDColor; 6] = [
        LEDColor::Blue,
        LEDColor::Teal,
        LEDColor::Green,
        LEDColor::Purple,
        LEDColor::Orange,
        LEDColor::Pink,
    ];

    loop {
        controller.boot_button.wait_for_falling_edge().await;

        if col_i >= colors.len() {
            col_i = 0;
        }

        let selected_color = colors[col_i];

        log::info!("Selected: {:?}", selected_color);

        controller
            .led
            .set(Some(selected_color), None, None, None)
            .await;

        controller
            .led
            .once(
                selected_color,
                Some(255),
                Some(Duration::from_millis(1000)),
                None,
            )
            .await;

        col_i += 1;

        Timer::after(Duration::from_millis(200)).await;
    }
}
