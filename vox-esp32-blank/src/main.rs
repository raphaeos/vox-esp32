use anyhow::Result;
use esp_idf_hal::{delay::FreeRtos};
use esp_idf_hal::gpio::PinDriver;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::sys::esp_random;
use std::thread::sleep;
use std::time::Duration;
use smart_leds::hsv::{hsv2rgb, Hsv};
use smart_leds_trait::SmartLedsWrite;
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

const LED_COLOR_GREEN: u8 = 85; // 520nm
const LET_COLOR_TEAL: u8 = 97; // 504nm

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    // 2. Take control of all hardware peripherals
    let peripherals = Peripherals::take()?;

    // 3. Configure the BOOT Button (GPIO0) as an Input with internal pull-up active
    let mut boot_button = PinDriver::input(peripherals.pins.gpio0, esp_idf_hal::gpio::Pull::Up)?;

    // Modern API Fix: Destructure the pin and the specific RMT channel peripheral type
    let led_pin = peripherals.pins.gpio48;
    #[allow(deprecated)]
    let rmt_channel = peripherals.rmt.channel0; 

    let mut ws2812 = Ws2812Esp32Rmt::new(rmt_channel, led_pin).unwrap();

    log::info!("System compiled cleanly with zero warnings.");


    // Test Green
    let pixels = std::iter::repeat(hsv2rgb(Hsv {
        hue: LET_COLOR_TEAL,
        sat: 255,
        val: 50,
    }))
    .take(25);
    ws2812.write(pixels).unwrap();

    /* 
    log::info!("Start NeoPixel rainbow!");

    let mut hue = unsafe { esp_random() } as u8;
    loop {
        let pixels = std::iter::repeat(hsv2rgb(Hsv {
            hue,
            sat: 255,
            val: 12,
        }))
        .take(25);
        ws2812.write(pixels).unwrap();

        sleep(Duration::from_millis(100));

        hue = hue.wrapping_add(10);
    }
    */

    Ok(())
}
