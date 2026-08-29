use crate::esp32::Peripherals;

use anyhow::{anyhow, Result};
use edge_executor::LocalExecutor;
#[cfg(feature = "esp32s3-rgb-led")]
use embassy_time::Duration;
use esp_idf_hal::gpio::{Input, PinDriver};
use futures::future::AbortHandle;
#[cfg(feature = "esp32s3-rgb-led")]
use smart_leds::{
    hsv::{hsv2rgb, Hsv},
    SmartLedsWrite, RGB8,
};
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(feature = "esp32s3-rgb-led")]
use ws2812_esp32_rmt_driver::{
    driver::color::LedPixelColorGrb24, LedPixelEsp32Rmt, Ws2812Esp32Rmt,
};

pub const BRIGHTNESS_DEFAULT: u8 = 50;
pub const BRIGHTNESS_MIN: u8 = 10;
pub const BRIGHTNESS_MAX: u8 = 255;
pub const BRIGHTNESS_OFF: u8 = 0;

#[cfg(feature = "esp32s3-rgb-led")]
#[derive(Clone, Copy, PartialEq)]
pub enum LEDStatus {
    On,
    Off,
    Blink,
}

#[cfg(feature = "esp32s3-rgb-led")]
#[derive(Clone, Copy, PartialEq)]
pub enum LEDColor {
    Blue,
    Teal,
    Green,
    Purple,
    Orange,
}

#[cfg(feature = "esp32s3-rgb-led")]
impl LEDColor {
    pub fn hue(&self) -> u8 {
        match self {
            LEDColor::Orange => 21,
            LEDColor::Green => 85,
            LEDColor::Teal => 97,
            LEDColor::Blue => 170,
            LEDColor::Purple => 192,
        }
    }
}

pub struct LEDDriver {
    ws2812: Arc<Mutex<LedPixelEsp32Rmt<'static, RGB8, LedPixelColorGrb24>>>,
}

impl LEDDriver {
    fn new(peripherals: &mut Peripherals) -> Result<Self> {
        let led_pin = peripherals.pins.gpio48.take().ok_or(anyhow!(
            "Controller: Failed to take LED pin, GPIO48 already taken"
        ))?;
        #[allow(deprecated)]
        let rmt_channel = peripherals.rmt.channel0.take().ok_or(anyhow!(
            "Controller: Failed to take RMT Channel, channel0 already taken"
        ))?;

        let ws2812 = Ws2812Esp32Rmt::new(rmt_channel, led_pin)?;

        Ok(Self {
            ws2812: Arc::new(Mutex::new(ws2812)),
        })
    }

    fn write(&self, hue: u8, sat: u8, val: u8) -> Result<()> {
        let pixels = std::iter::repeat(hsv2rgb(Hsv { hue, sat, val })).take(1);

        let mut driver = self
            .ws2812
            .lock()
            .map_err(|e| anyhow!("Failed to lock LED driver for Off state: {:?}", e))?;
        driver.write(pixels)?;

        Ok(())
    }
}

#[cfg(feature = "esp32s3-rgb-led")]
pub struct LED {
    color: LEDColor,
    brightness: u8,
    status: LEDStatus,
    driver: Arc<LEDDriver>,
    blink_aborter: Option<AbortHandle>,
}

#[cfg(feature = "esp32s3-rgb-led")]
impl LED {
    fn new(peripherals: &mut Peripherals) -> Result<Self> {
        Ok(Self {
            color: LEDColor::Green,
            brightness: BRIGHTNESS_DEFAULT,
            status: LEDStatus::Off,
            driver: Arc::new(LEDDriver::new(peripherals)?),
            blink_aborter: None,
        })
    }

    fn set(
        &mut self,
        executor: &edge_executor::LocalExecutor<'static>,
        color: LEDColor,
        status: LEDStatus,
        blink_delay: Option<Duration>,
    ) -> Result<()> {
        if let Some(aborter) = self.blink_aborter.take() {
            aborter.abort();
        }

        if status == LEDStatus::Blink {
            let duration = blink_delay.unwrap_or(Duration::from_millis(500));
            let brightness = self.brightness;
            let driver = self.driver.clone();

            let (splittable_fut, aborter) = futures::future::abortable(async move {
                let mut is_on = true;
                loop {
                    let cur_brightness = if is_on { brightness } else { BRIGHTNESS_OFF };

                    //self.write(color.hue(), 255, cur_brightness)?;

                    is_on = !is_on;
                    embassy_time::Timer::after(duration).await;
                }

                Ok::<(), anyhow::Error>(())
            });

            executor
                .spawn(async move {
                    let _ = splittable_fut.await;
                })
                .detach();

            self.blink_aborter = Some(aborter);
        } else {
            let brightness = match status {
                LEDStatus::Off => BRIGHTNESS_OFF,
                _ => self.brightness,
            };

            self.driver.write(color.hue(), 255, brightness)?;
        }

        self.color = color;
        self.status = status;

        Ok(())
    }
}

pub struct Controller {
    pub peripherals: Peripherals,
    pub executor: LocalExecutor<'static>,
    pub boot_button: PinDriver<'static, Input>,
    #[cfg(feature = "esp32s3-rgb-led")]
    pub led: LED,
}

impl Controller {
    pub(crate) fn new(mut peripherals: Peripherals) -> Result<Self> {
        let executor = LocalExecutor::new();

        let boot_pin = peripherals.pins.gpio0.take().ok_or(anyhow!(
            "Controller: Failed to take boot pin, GPIO0 already taken"
        ))?;
        let mut boot_button = PinDriver::input(boot_pin, esp_idf_hal::gpio::Pull::Up)?;

        #[cfg(feature = "esp32s3-rgb-led")]
        let led = LED::new(&mut peripherals)?;

        Ok(Self {
            peripherals,
            executor,
            boot_button,
            #[cfg(feature = "esp32s3-rgb-led")]
            led,
        })
    }

    pub(crate) fn setup() -> Result<Self> {
        Self::new(Peripherals::new(
            esp_idf_hal::peripherals::Peripherals::take()?,
        ))
    }
}
