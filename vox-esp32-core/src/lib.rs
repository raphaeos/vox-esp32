#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]
extern crate alloc;

#[cfg(feature = "ads111x")]
pub mod ads111x;
pub mod async_channel;
pub mod common;
pub mod controller;
pub mod esp32;
#[cfg(feature = "esp32s3-adc")]
pub mod esp32_adc;
#[cfg(feature = "esp32s3-can")]
pub mod esp32_can;
#[cfg(feature = "esp32s3-rgb-led")]
pub mod esp32_led;
pub mod types;

pub use controller::Controller;
pub use esp32::Peripherals;

use anyhow::Result;
use core::panic::PanicInfo;
use embassy_executor::Spawner;
use embedded_hal::delay::DelayNs;
#[cfg(feature = "esp32s3-can")]
pub use esp32_can::{CANManagerHandle, CANRxHandler, MessageId, MessageType};
use esp_hal::time::Duration;
use esp_hal::{delay::Delay, system::software_reset};
pub use types::*;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    esp_println::println!("PANIC: {info}");
    esp_println::println!("Restarting in 10 seconds...");

    let mut delay = Delay::new();
    delay.delay_ms(10_000);

    software_reset();
}

pub fn init(device: Device, spawner: Spawner, wdt_timeout: Option<Duration>) -> Result<Controller> {
    esp_println::logger::init_logger_from_env();

    log::info!("Vox ESP32 Core: Initializing");

    Controller::setup(device, spawner, wdt_timeout)
}
