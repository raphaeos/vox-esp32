#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use anyhow::Result;

pub use controller::Controller;
pub use esp32::Peripherals;

pub mod common;
pub mod controller;
pub mod esp32;
#[cfg(feature = "esp32s3-rgb-led")]
pub mod esp32_led;

pub fn init() -> Result<Controller> {
    log::info!("Vox ESP32 Core: Initializing");

    Controller::setup()
}
