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
use esp_hal::analog::adc::{AdcCalScheme, AdcHasCurveCal};
use vox_esp32_core::esp32_led::{LEDColor, LEDStatus};

mod powmr_mppt;

/**

GPIO Notes:
    GPIO 17: Serial Module - TX
    GPIO 18: Serial Module - RX

**/

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

    //let mppt_rx = MPPTManager::spawn(&mut controller)?;

    controller
        .led
        .set(Some(LEDColor::Teal), Some(LEDStatus::Blink), None, None)
        .await;

    loop {
        Timer::after(Duration::from_millis(1000)).await;
    }
}
