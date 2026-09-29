#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

pub mod app;
pub mod can;
pub mod display;
pub mod ui;

extern crate alloc;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::system::software_reset;
use vox_esp32_core::esp32_led::{LEDColor, LEDStatus};
use vox_esp32_core::types::{Device, DeviceType};
use vox_esp32_core::Controller;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let mut controller = vox_esp32_core::init(
        Device::new(DeviceType::Control, 1),
        spawner,
        Some(esp_hal::time::Duration::from_millis(2000)),
    )
    .expect("failed to initialize ESP controller");

    controller
        .led
        .set(Some(LEDColor::Green), Some(LEDStatus::Blink), None, None)
        .await;

    log::info!("Vox ESP32 Control: Started");

    if let Err(e) = init(&mut controller).await {
        log::error!("Error in main init: {:?}", e);

        Timer::after(Duration::from_millis(5000)).await;
        software_reset();
    } else {
        loop {
            // TODO:
            controller.feed();

            // Endless loop.
            Timer::after(Duration::from_millis(1000)).await;
        }
    }
}

pub async fn init(controller: &mut Controller) -> anyhow::Result<()> {
    let app_update_tx = app::start(controller).await?;
    can::start(controller, app_update_tx).await
}
