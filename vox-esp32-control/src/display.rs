/*
   #	Pin Label	Description
   1	VCC         Power positive(It is recommended to connect to 5V. When connected to 3.3V, the backlight brightness will be slightly dim)
        (5V red)
   2	GND	        Power ground
        (GND brown)
   3	LCD_CS	    LCD selection control signal, Low level active
        GPIO 1 (orange)
   4	LCD_RST	    LCD reset control signal, Low level reset
        GPIO 2 (yellow)
   5	LCD_RS	    LCD command / data selection control signal
                    High level: data, low level: command
        GPIO 42 (green)
   6	SDI(MOSI)	SPI bus write data signal(SD card and LCD screen used together)
        GPIO 41 (blue)
   7	SCK	        SPI bus clock signal(SD card and LCD screen used together)
        GPIO 40 (purple)
   8	LED	        LCD backlight control signal (If you need control, please connect the pins. If you don't need control, you can skip it)
        GPIO 39 (grey)
   9	SDO(MISO)	SPI bus read data signal (SD card and LCD screen used together)
        GPIO 38 (white)

        (skipped GPIO's for safety, GPIO 48 is used by the onboard WLED)

   10	CTP_SCL	    Capacitive touch screen IIC bus clock signal (modules without touch screens do not need to be connected)
        GPIO 47 (black)
   11	CTP_RST	    Capacitor touch screen reset control signal, low-level reset (modules without touch screens do not need to be connected)
        GPIO 21 (brown)
   12	CTP_SDA	    Capacitive touch screen IIC bus data signal (modules without touch screens do not need to be connected)
        GPIO 20 (red)
   13	CTP_INT	    Capacitor touch screen IIC bus touch interrupt signal, when generating touch, input low level to the main control (modules without touch screens do not need to be connected)
        GPIO 19 (orange)
   14	SD_CS	    SD card selection control signal, low level active (without SD card function, can be disconnected)
        NOT USED
*/
use alloc::rc::Rc;
use alloc::vec::Vec;
use anyhow::Result;
use embassy_time::{Delay, Duration, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::gpio::Pin;
use esp_hal::gpio::{Input, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    spi::{
        master::{Config, Spi},
        Mode,
    },
    time::Rate,
    Async,
};
use ft6336u_dd::{Ft6336uAsync, Ft6336uInterface};
use lcd_async::interface::SpiInterface;
use lcd_async::models::ST7796;
use lcd_async::{Builder, Display};
use vox_esp32_core::common::CoreError;
use vox_esp32_core::Controller;

use crate::ui::AppWindow;
use core::ops::Range;
use esp_hal::ledc::channel::Channel;
use lcd_async::options::{ColorOrder, Orientation, Rotation};
use slint::platform::{
    software_renderer::{LineBufferProvider, MinimalSoftwareWindow, Rgb565Pixel},
    PointerEventButton, WindowEvent,
};
use slint::LogicalPosition;
use static_cell::StaticCell;

pub type DisplayAdapter = Display<
    SpiInterface<ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>, Output<'static>>,
    ST7796,
    Output<'static>,
>;
pub type TouchAdapter =
    Ft6336uAsync<Ft6336uInterface<I2c<'static, Async>>, esp_hal::i2c::master::Error>;

static LEDC_TIMER: StaticCell<timer::Timer<'static, LowSpeed>> = StaticCell::new();

pub const DISPLAY_WIDTH: u16 = 320;
pub const DISPLAY_HEIGHT: u16 = 480;
pub const UI_WIDTH: u16 = 480;
pub const UI_HEIGHT: u16 = 320;

struct Framebuffer<'a> {
    pixels: &'a mut [Rgb565Pixel],
}

impl<'a> LineBufferProvider for Framebuffer<'a> {
    type TargetPixel = Rgb565Pixel;

    fn process_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        render_fn: impl FnOnce(&mut [Rgb565Pixel]),
    ) {
        let start = line * (UI_WIDTH as usize) + range.start;
        let end = line * (UI_WIDTH as usize) + range.end;

        render_fn(&mut self.pixels[start..end]);
    }
}

async fn init_display(
    controller: &mut Controller,
) -> Result<(DisplayAdapter, Channel<'static, LowSpeed>)> {
    let device_delay = Delay;
    let mut init_delay = Delay;

    // 1. Set up standard outputs for your display control lines
    let tft_cs = Output::new(
        controller
            .peripherals
            .GPIO1
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO1"))?,
        Level::High,
        OutputConfig::default(),
    );
    let tft_rst = Output::new(
        controller
            .peripherals
            .GPIO2
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO2"))?,
        Level::High,
        OutputConfig::default(),
    );
    let tft_dc = Output::new(
        controller
            .peripherals
            .GPIO42
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO42"))?,
        Level::Low,
        OutputConfig::default(),
    );
    let bl_pin = controller
        .peripherals
        .GPIO39
        .take()
        .ok_or(CoreError::PeripheralTaken("GPIO39"))?
        .degrade();

    let mut ledc = Ledc::new(
        controller
            .peripherals
            .LEDC
            .take()
            .ok_or(CoreError::PeripheralTaken("LEDC"))?,
    );
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut timer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    timer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty10Bit, // 0 to 1023 resolution steps
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(50),
        })
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let static_timer = LEDC_TIMER.init(timer0);

    let mut tft_bl = ledc.channel(channel::Number::Channel0, bl_pin);
    tft_bl
        .configure(channel::config::Config {
            timer: static_timer,
            duty_pct: 0, // Turn off to begin with.
            drive_mode: esp_hal::gpio::DriveMode::PushPull,
        })
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let spi_config = Config::default()
        .with_frequency(Rate::from_mhz(40))
        .with_mode(Mode::_0);

    let spi = Spi::new(
        (&mut controller.peripherals.SPI2)
            .take()
            .ok_or(CoreError::PeripheralTaken("SPI2"))?,
        spi_config,
    )? // Standard esp-hal result unpacker
    .with_sck(
        (&mut controller.peripherals.GPIO40)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO40"))?,
    )
    .with_mosi(
        (&mut controller.peripherals.GPIO41)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO41"))?,
    )
    .with_miso(
        (&mut controller.peripherals.GPIO38)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO38"))?,
    )
    .into_async();

    let spi_device = ExclusiveDevice::new(spi, tft_cs, device_delay)?;
    let di = SpiInterface::new(spi_device, tft_dc);

    let mut display = Builder::new(ST7796, di)
        .reset_pin(tft_rst)
        .color_order(ColorOrder::Bgr)
        .display_size(DISPLAY_WIDTH, DISPLAY_HEIGHT)
        .init(&mut init_delay)
        .await
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    display
        .set_orientation(Orientation::new().flip_horizontal().rotate(Rotation::Deg90))
        .await
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    Ok((display, tft_bl))
}

async fn init_touch(controller: &mut Controller) -> Result<(TouchAdapter, Input<'static>)> {
    // Setup touch
    let mut ctp_rst = Output::new(
        (&mut controller.peripherals.GPIO21)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO21"))?,
        Level::Low,
        OutputConfig::default(),
    );
    Timer::after(Duration::from_millis(10)).await;
    ctp_rst.set_high(); // Release reset line
    Timer::after(Duration::from_millis(50)).await;

    let i2c_config = I2cConfig::default().with_frequency(Rate::from_khz(400)); // Standard Fast-Mode I2C

    let i2c = I2c::new(
        (&mut controller.peripherals.I2C0)
            .take()
            .ok_or(CoreError::PeripheralTaken("I2C0"))?,
        i2c_config,
    )?
    .with_scl(
        (&mut controller.peripherals.GPIO47)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO47"))?,
    ) // black
    .with_sda(
        (&mut controller.peripherals.GPIO20)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO20"))?,
    ) // red
    .into_async(); // Converts driver to Embassy async non-blocking execution

    let touch_int = Input::new(
        (&mut controller.peripherals.GPIO19)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO19"))?,
        esp_hal::gpio::InputConfig::default().with_pull(Pull::Up),
    );

    Ok((Ft6336uAsync::new(i2c), touch_int))
}

#[embassy_executor::task]
async fn display_task(mut mgr: DisplayManager) {
    mgr.run().await;
}

pub struct DisplayManager {
    display: DisplayAdapter,
    tft_bl: Channel<'static, LowSpeed>,
    window: Rc<MinimalSoftwareWindow>,
    render_buffer: Vec<Rgb565Pixel>,
    transfer_buffer: Vec<u8>,
    screen_on: bool,
}

impl DisplayManager {
    pub(crate) async fn spawn(
        controller: &mut Controller,
        window: Rc<MinimalSoftwareWindow>,
    ) -> Result<()> {
        let (mut display, mut tft_bl) = init_display(controller).await?;

        let mut render_buffer = Vec::with_capacity(UI_WIDTH as usize * UI_HEIGHT as usize);
        render_buffer.resize(UI_WIDTH as usize * UI_HEIGHT as usize, Rgb565Pixel(0));

        let mut transfer_buffer = Vec::with_capacity(UI_WIDTH as usize * UI_HEIGHT as usize * 2); // 2 bytes per pixel
        transfer_buffer.resize(UI_WIDTH as usize * UI_HEIGHT as usize * 2, 0u8);

        let mgr = DisplayManager {
            display,
            tft_bl,
            window,
            render_buffer,
            transfer_buffer,
            screen_on: false,
        };

        controller.spawn(display_task(mgr)?);

        Ok(())
    }

    async fn run(&mut self) {
        log::info!("DisplayManager: Started");

        loop {
            if let Err(e) = self.draw().await {
                log::error!("DisplayManager draw failed: {:?}", e);
                Timer::after(Duration::from_millis(100)).await;
            } else {
                Timer::after(Duration::from_millis(16)).await;
            }
        }
    }

    async fn draw(&mut self) -> Result<()> {
        // Pump Slint's clock ticks so animations and property state systems update
        slint::platform::update_timers_and_animations();

        // 1. Immediate-mode check: only touch the frame memory if Slint actively redraws it
        let mut ui_did_render = false;
        self.window.draw_if_needed(|renderer| {
            renderer.render_by_line(Framebuffer {
                pixels: self.render_buffer.as_mut_slice(),
            });
            ui_did_render = true;
        });

        if ui_did_render {
            let raw_render_bytes = unsafe {
                core::slice::from_raw_parts(
                    self.render_buffer.as_ptr() as *const u8,
                    self.render_buffer.len() * size_of::<Rgb565Pixel>(),
                )
            };

            for (i, chunk) in raw_render_bytes.chunks_exact(2).enumerate() {
                self.transfer_buffer[i * 2] = chunk[1]; // Swap byte orders cleanly
                self.transfer_buffer[i * 2 + 1] = chunk[0]; // No bit inversions or hacks
            }

            self.display
                .show_raw_data(0, 0, UI_WIDTH, UI_HEIGHT, self.transfer_buffer.as_slice())
                .await
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;

            if !self.screen_on {
                // Turn on screen
                self.tft_bl
                    .set_duty(50)
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;
                self.screen_on = true;
            }
        }

        Ok(())
    }
}

#[embassy_executor::task]
async fn touch_task(mut mgr: TouchManager) {
    mgr.run().await;
}

pub struct TouchManager {
    touch: TouchAdapter,
    touch_int: Input<'static>,
    window: Rc<MinimalSoftwareWindow>,
    touch_active: bool,
    last_x: f32,
    last_y: f32,
}

impl TouchManager {
    pub(crate) async fn spawn(
        controller: &mut Controller,
        window: Rc<MinimalSoftwareWindow>,
    ) -> Result<()> {
        let (mut touch, mut touch_int) = init_touch(controller).await?;

        let mgr = TouchManager {
            touch,
            touch_int,
            window,
            touch_active: false,
            last_x: 0.0,
            last_y: 0.0,
        };

        controller.spawn(touch_task(mgr)?);

        Ok(())
    }

    async fn run(&mut self) {
        log::info!("TouchManager: Started");

        loop {
            if let Err(e) = self.scan().await {
                log::error!("TouchManager scan failed: {:?}", e);
                Timer::after(Duration::from_millis(100)).await;
            }
        }
    }

    async fn scan(&mut self) -> Result<()> {
        self.touch_int.wait_for_low().await;

        if let Ok(data) = self.touch.scan().await {
            let pressed = data.touch_count > 0;

            if pressed {
                // Extract raw integers from your FT6336U scan payload
                let raw_x = data.points[0].x as i32;
                let raw_y = data.points[0].y as i32;

                // Apply the landscape rotation translation matrix natively
                // This maps your portrait coordinates directly to Slint's pixel grid
                let calibrated_x = raw_y as f32;
                let calibrated_y = (320 - raw_x) as f32;

                if self.last_x == calibrated_x && self.last_y == calibrated_y {
                    // Skip dupe frames
                    return Ok(());
                }

                log::trace!(
                    "TouchManager: Touch pressed={}, x={}, y={}",
                    pressed,
                    calibrated_x,
                    calibrated_y
                );

                if !self.touch_active {
                    self.window.dispatch_event(WindowEvent::PointerPressed {
                        position: LogicalPosition::new(calibrated_x, calibrated_y),
                        button: PointerEventButton::Left,
                    });

                    self.touch_active = true;
                } else {
                    self.window.dispatch_event(WindowEvent::PointerMoved {
                        position: LogicalPosition::new(calibrated_x, calibrated_y),
                    });
                }

                self.last_x = calibrated_x;
                self.last_y = calibrated_y;
            } else if self.touch_active {
                log::trace!("TouchManager: Touch pressed={}", pressed);

                self.window.dispatch_event(WindowEvent::PointerReleased {
                    position: LogicalPosition::new(self.last_x, self.last_y),
                    button: PointerEventButton::Left,
                });

                self.touch_active = false;
            }
        }

        Ok(())
    }
}

pub async fn spawn(
    controller: &mut Controller,
    window: Rc<MinimalSoftwareWindow>,
    app: AppWindow,
) -> Result<()> {
    DisplayManager::spawn(controller, window.clone()).await?;
    TouchManager::spawn(controller, window).await
}
