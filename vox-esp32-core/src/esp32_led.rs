use std::sync::Arc;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use async_channel::Sender;
use async_channel::{unbounded, Receiver};
use edge_executor::{LocalExecutor, Task};
use embassy_time::Duration;
use embassy_time::Timer;
use futures::future::{self};
use futures::pin_mut;
use futures::select_biased;
use futures::FutureExt;
use smart_leds::{
    hsv::{hsv2rgb, Hsv},
    SmartLedsWrite, RGB8,
};
use ws2812_esp32_rmt_driver::{
    driver::color::LedPixelColorGrb24, LedPixelEsp32Rmt, Ws2812Esp32Rmt,
};

use crate::esp32::Peripherals;

pub const BRIGHTNESS_DEFAULT: u8 = 20;
pub const BRIGHTNESS_MIN: u8 = 10;
pub const BRIGHTNESS_MAX: u8 = 255;
pub const BRIGHTNESS_OFF: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LEDMode {
    Hsv,
    Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LEDStatus {
    On,
    Off,
    Blink,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LEDColor {
    Blue,
    Teal,
    Green,
    Purple,
    Orange,
    Pink,
}

impl LEDColor {
    pub fn hue(&self) -> u8 {
        match self {
            LEDColor::Blue => 170,
            LEDColor::Teal => 97,
            LEDColor::Green => 85,
            LEDColor::Purple => 182,
            LEDColor::Orange => 21,
            LEDColor::Pink => 234,
        }
    }

    pub fn rgb(&self) -> RGB8 {
        match self {
            LEDColor::Blue => RGB8::new(0, 0, 255),
            LEDColor::Teal => RGB8::new(0, 150, 150),
            LEDColor::Green => RGB8::new(0, 255, 0),
            LEDColor::Purple => RGB8::new(90, 15, 255),
            LEDColor::Orange => RGB8::new(255, 45, 0),
            LEDColor::Pink => RGB8::new(255, 0, 110),
        }
    }
}

pub struct LEDDriver {
    ws2812: Arc<Mutex<LedPixelEsp32Rmt<'static, RGB8, LedPixelColorGrb24>>>,
    mode: LEDMode,
}

impl LEDDriver {
    fn new(mode: LEDMode, peripherals: &mut Peripherals) -> Result<Self> {
        let led_pin = peripherals.pins.gpio48.take().ok_or(anyhow!(
            "Controller: Failed to take LED pin, GPIO48 already taken"
        ))?;
        #[allow(deprecated)]
        let rmt_channel = peripherals.rmt.channel0.take().ok_or(anyhow!(
            "Controller: Failed to take RMT Channel, channel0 already taken"
        ))?;

        let ws2812 = Ws2812Esp32Rmt::new(rmt_channel, led_pin)?;

        Ok(Self {
            mode,
            ws2812: Arc::new(Mutex::new(ws2812)),
        })
    }

    fn write(&self, color: LEDColor, saturation: u8, brightness: u8) -> Result<()> {
        let pixels = match self.mode {
            LEDMode::Hsv => [hsv2rgb(Hsv {
                hue: color.hue(),
                sat: saturation,
                val: brightness,
            })]
            .into_iter(),
            LEDMode::Rgb => [apply_rgb_brightness(color.rgb(), brightness)].into_iter(),
        };

        let mut driver = self
            .ws2812
            .lock()
            .map_err(|e| anyhow!("Failed to lock LED driver for Off state: {:?}", e))?;
        driver.write(pixels)?;

        Ok(())
    }
}

pub struct LED {
    color: LEDColor,
    brightness: u8,
    status: LEDStatus,
    blink_delay: Duration,
    blink_state: bool,
    driver: Arc<LEDDriver>,
}

impl LED {
    fn new(peripherals: &mut Peripherals) -> Result<Self> {
        Ok(Self {
            color: LEDColor::Green,
            brightness: BRIGHTNESS_DEFAULT,
            blink_delay: Duration::from_millis(500),
            blink_state: false,
            status: LEDStatus::Off,
            driver: Arc::new(LEDDriver::new(LEDMode::Rgb, peripherals)?),
        })
    }

    fn update(&mut self) -> Result<()> {
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

        (*self.driver).write(self.color, 255, brightness)?;

        Ok(())
    }

    fn toggle_blink(&mut self) -> Result<()> {
        if self.status == LEDStatus::Blink {
            self.blink_state = !self.blink_state;
            self.update()
        } else {
            Err(anyhow!(
                "toggle_blink called when status != LEDStatus::Blink"
            ))
        }
    }
}

#[derive(Debug)]
struct LEDSetMessage {
    color: Option<LEDColor>,
    status: Option<LEDStatus>,
    brightness: Option<u8>,
    blink_delay: Option<Duration>,
}

#[derive(Debug)]
struct LEDOnceMessage {
    color: LEDColor,
    brightness: Option<u8>,
    duration: Option<Duration>,
    resume_status: Option<LEDStatus>,
}

#[derive(Debug)]
enum LEDManagerMessage {
    Set(LEDSetMessage),
    Once(LEDOnceMessage),
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
    ) -> &Self {
        let _ = self
            .tx
            .send(LEDManagerMessage::Set(LEDSetMessage {
                color,
                status,
                brightness,
                blink_delay,
            }))
            .await;
        self
    }

    pub async fn status(&self, status: LEDStatus) -> &Self {
        let _ = self
            .tx
            .send(LEDManagerMessage::Set(LEDSetMessage {
                color: None,
                status: Some(status),
                brightness: None,
                blink_delay: None,
            }))
            .await;
        self
    }

    pub async fn on(&self) -> &Self {
        self.status(LEDStatus::On).await
    }

    pub async fn off(&self) -> &Self {
        self.status(LEDStatus::Off).await
    }

    pub async fn once(
        &self,
        color: LEDColor,
        brightness: Option<u8>,
        duration: Option<Duration>,
        resume_status: Option<LEDStatus>,
    ) -> &Self {
        let _ = self
            .tx
            .send(LEDManagerMessage::Once(LEDOnceMessage {
                color,
                brightness,
                duration,
                resume_status,
            }))
            .await;
        self
    }
}

pub struct LEDManager;

impl LEDManager {
    pub(crate) fn spawn(
        executor: &mut LocalExecutor<'static>,
        peripherals: &mut Peripherals,
    ) -> Result<(LEDManagerHandle, Task<()>)> {
        let mut led = LED::new(peripherals)?;

        let (tx, rx) = unbounded::<LEDManagerMessage>();

        // Spawn the event loop on edge-executor
        let task = executor.spawn(async move {
            log::info!("Vox ESP32 Core: LED Manager started");

            Self::run(rx, &mut led).await
        });

        Ok((LEDManagerHandle { tx }, task))
    }

    async fn run(rx: Receiver<LEDManagerMessage>, led: &mut LED) {
        loop {
            let recv_fut = rx.recv().fuse();

            let timeout_fut = if led.status == LEDStatus::Blink {
                Timer::after(led.blink_delay).left_future()
            } else {
                future::pending::<()>().right_future()
            }
            .fuse();

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
            LEDManagerMessage::Once(msg) => {
                let mut last_status = led.status;
                let last_color = led.color;
                let last_brightness = led.brightness;

                if let Some(resume_status) = msg.resume_status {
                    last_status = resume_status;
                }

                // Set temporary values
                led.status = LEDStatus::On;
                led.color = msg.color;
                if let Some(brightness) = msg.brightness {
                    led.brightness = brightness;
                }

                // Flash
                if let Err(e) = led.update() {
                    log::error!("Vox ESP32 Core: Failed to update LED: {e}")
                }

                Timer::after(if let Some(duration) = msg.duration {
                    duration
                } else {
                    led.blink_delay
                })
                .await;

                // Revert
                led.status = last_status;
                led.color = last_color;
                led.brightness = last_brightness;

                // Restore
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

// Utils

pub fn apply_rgb_brightness(base_rgb: RGB8, brightness: u8) -> RGB8 {
    RGB8::new(
        ((base_rgb.r as u16 * brightness as u16) / 255) as u8,
        ((base_rgb.g as u16 * brightness as u16) / 255) as u8,
        ((base_rgb.b as u16 * brightness as u16) / 255) as u8,
    )
}
