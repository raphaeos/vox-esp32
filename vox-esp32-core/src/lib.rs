use anyhow::Result;

pub use controller::Controller;
pub use esp32::{Peripherals, Pins, RMT};

pub mod controller;
pub mod esp32;
#[cfg(feature = "esp32s3-rgb-led")]
pub mod esp32_led;

pub fn init() -> Result<Controller> {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Vox ESP32 Core: Initializing");

    Controller::setup()
}
