#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    prelude::*,
};

#[entry]
fn main() -> ! {
    // Take ownership of the peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize system delay using the hardware clock
    let delay = Delay::new();

    // Loop forever printing to the native USB-Serial console
    loop {
        esp_hal::println!("Vox-ESP32 Grid-Down System Online!");
        delay.delay_millis(1000);
    }
}
