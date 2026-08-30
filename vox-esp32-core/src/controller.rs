use std::sync::Arc;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use async_channel::{Receiver, unbounded};
use async_channel::Sender;
use edge_executor::LocalExecutor;
use embassy_time::Duration;
#[cfg(feature = "esp32s3-rgb-led")]
use embassy_time::Timer;
use esp_idf_hal::gpio::{Input, PinDriver};
use futures::future::{self};
use futures::FutureExt;
#[cfg(feature = "esp32s3-rgb-led")]
use futures::pin_mut;
#[cfg(feature = "esp32s3-rgb-led")]
use futures::select_biased;
#[cfg(feature = "esp32s3-rgb-led")]
use smart_leds::{
    hsv::{Hsv, hsv2rgb},
    RGB8, SmartLedsWrite,
};
#[cfg(feature = "esp32s3-rgb-led")]
use ws2812_esp32_rmt_driver::{
    driver::color::LedPixelColorGrb24, LedPixelEsp32Rmt, Ws2812Esp32Rmt,
};

use crate::esp32::Peripherals;

pub const BRIGHTNESS_DEFAULT: u8 = 12;
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
        let pixels = [hsv2rgb(Hsv { hue, sat, val })].into_iter();

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
    blink_delay: Duration,
    blink_state: bool,
    driver: Arc<LEDDriver>,
}

#[cfg(feature = "esp32s3-rgb-led")]
impl LED {
    fn new(peripherals: &mut Peripherals) -> Result<Self> {
        Ok(Self {
            color: LEDColor::Green,
            brightness: BRIGHTNESS_DEFAULT,
            blink_delay: Duration::from_millis(500),
            blink_state: false,
            status: LEDStatus::Off,
            driver: Arc::new(LEDDriver::new(peripherals)?),
        })
    }

    fn update(
        &mut self,
    ) -> Result<()> {
        let brightness = match self.status {
            LEDStatus::Off => BRIGHTNESS_OFF,
            LEDStatus::Blink => {
                if self.blink_state {
                    self.brightness
                } else {
                    BRIGHTNESS_OFF
                }
            }
            _ => self.brightness,
        };

        (*self.driver).write(self.color.hue(), 255, brightness)?;

        Ok(())
    }

    fn toggle_blink(
        &mut self,
    ) -> Result<()> {
        if self.status == LEDStatus::Blink {
            self.blink_state = !self.blink_state;
            self.update()
        } else {
            Err(anyhow!("toggle_blink called when status != LEDStatus::Blink"))
        }
    }
}

#[cfg(feature = "esp32s3-rgb-led")]
struct LEDSetMessage {
    color: Option<LEDColor>,
    status: Option<LEDStatus>,
    brightness: Option<u8>,
    blink_delay: Option<Duration>,
}

#[cfg(feature = "esp32s3-rgb-led")]
enum LEDManagerMessage {
    Set(LEDSetMessage),
}

#[derive(Clone)]
pub struct LEDManagerHandle {
    tx: Sender<LEDManagerMessage>,
}

impl LEDManagerHandle {
    pub async fn set(
        &self,
        color: Option<LEDColor>,
        status: Option<LEDStatus>,
        brightness: Option<u8>,
        blink_delay: Option<Duration>,
    ) {
        let _ = self
            .tx
            .send(LEDManagerMessage::Set(LEDSetMessage {
                color,
                status,
                brightness,
                blink_delay,
            }))
            .await;
    }

    pub async fn status(&self, status: LEDStatus) {
        let _ = self
            .tx
            .send(LEDManagerMessage::Set(LEDSetMessage {
                color: None,
                status: Some(status),
                brightness: None,
                blink_delay: None,
            }))
            .await;
    }

    pub async fn on(&self) {
        self.status(LEDStatus::On).await
    }

    pub async fn off(&self) {
        self.status(LEDStatus::Off).await
    }
}

#[cfg(feature = "esp32s3-rgb-led")]
pub struct LEDManager;

#[cfg(feature = "esp32s3-rgb-led")]
impl LEDManager {
    fn spawn(executor: &mut LocalExecutor<'static>, mut led: LED) -> LEDManagerHandle {
        let (tx, rx) = unbounded::<LEDManagerMessage>();
        
        // Spawn the event loop on edge-executor
        let _ = executor.spawn(async move {
            log::info!("Vox ESP32 Core: LED Manager started");

            Self::run(rx, &mut led).await;
        });

        LEDManagerHandle { tx }
    }

    async fn run(rx: Receiver<LEDManagerMessage>, led: &mut LED) {
        loop {
            let mut recv_fut = rx.recv().fuse();

            let mut timeout_fut = if led.status == LEDStatus::Blink {
                Timer::after(led.blink_delay).left_future()
            } else {
                future::pending::<()>().right_future()
            }.fuse();

            pin_mut!(recv_fut, timeout_fut);

            select_biased! {
                msg_res = recv_fut => {
                    match msg_res {
                        Ok(msg) => {
                            Self::process_msg(msg, led).await;
                        }
                        Err(_) => {
                            // Channel was closed, exit the loop safely
                            break;
                        }
                    }
                },
                _ = timeout_fut => {
                    Self::process_timeout(led).await;
                }
            }
        }
    }

    async fn process_msg(msg: LEDManagerMessage, led: &mut LED) {
        match msg {
            LEDManagerMessage::Set(msg) => {
                if let Some(color) = msg.color {
                    led.color = color;
                }
                if let Some(status) = msg.status {
                    led.status = status;
                }
                if let Some(brightness) = msg.brightness {
                    led.brightness = brightness;
                }
                if let Some(blink_delay) = msg.blink_delay {
                    led.blink_delay = blink_delay;
                }

                if let Err(e) = led.update() {
                    log::error!("Vox ESP32 Core: Failed to update LED: {e}")
                }
            }
        }
    }

    async fn process_timeout(led: &mut LED) {
        if led.status == LEDStatus::Blink {
            if let Err(e) = led.toggle_blink() {
                log::error!("Vox ESP32 Core: Failed to toggle LED blink: {e}")
            }
        }
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
        let boot_button = PinDriver::input(boot_pin, esp_idf_hal::gpio::Pull::Up)?;

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
