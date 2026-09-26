/*
   #	Pin Label	Description
   1	VCC         Power positive(It is recommended to connect to 5V. When connected to 3.3V, the backlight brightness will be slightly dim)
        (5V red)
   2	GND	        Power ground
        (GND brown)
   3	LCD_CS	    LCD selection control signal, Low level active
        GPIO 10 (orange)
   4	LCD_RST	    LCD reset control signal, Low level reset
        GPIO 2 (yellow)
   5	LCD_RS	    LCD command / data selection control signal
                    High level: data, low level: command
        GPIO 42 (green)
   6	SDI(MOSI)	SPI bus write data signal(SD card and LCD screen used together)
        GPIO 11 (blue)
   7	SCK	        SPI bus clock signal(SD card and LCD screen used together)
        GPIO 12 (purple)
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
use esp_hal::time::Instant;
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
    Async, Blocking,
};
use ft6336u_dd::{Ft6336uAsync, Ft6336uInterface};
use mipidsi::{
    interface::SpiInterface,
    models::ST7796,
    options::{ColorOrder, Orientation, Rotation},
    Builder,
};
use vox_esp32_core::common::CoreError;
use vox_esp32_core::Controller;

use crate::ui::AppWindow;
use core::ops::Range;
use esp_hal::ledc::channel::Channel;
use slint::platform::{
    software_renderer::{LineBufferProvider, MinimalSoftwareWindow, Rgb565Pixel},
    PointerEventButton, WindowEvent,
};
use slint::LogicalPosition;
use static_cell::StaticCell;

pub type DisplayAdapter = mipidsi::Display<
    SpiInterface<
        'static,
        ExclusiveDevice<Spi<'static, Blocking>, Output<'static>, Delay>,
        Output<'static>,
    >,
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

struct SyncLineBuffer<'a> {
    display: &'a mut DisplayAdapter,
    chunk_buffer: &'a mut [u16], // Use u16 directly to match Slint's internal pixel size
}

impl<'a> LineBufferProvider for SyncLineBuffer<'a> {
    type TargetPixel = Rgb565Pixel;

    fn process_line(
        &mut self,
        line_index: usize,
        range: Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        let pixel_count = range.end - range.start;

        // Calculate where this specific line belongs inside our 16-line chunk buffer
        let chunk_line_offset = line_index % 16;
        let start_idx = chunk_line_offset * pixel_count;
        let end_idx = start_idx + pixel_count;

        let pool = unsafe {
            core::slice::from_raw_parts_mut(
                self.chunk_buffer[start_idx..end_idx].as_mut_ptr() as *mut Self::TargetPixel,
                pixel_count,
            )
        };

        // Render directly into the correct row of our internal SRAM chunk
        render_fn(pool);

        // If we have filled up 16 lines, OR we have reached the very last line of the screen, blast the batch
        if chunk_line_offset == 15 || line_index == (UI_HEIGHT as usize - 1) {
            let total_lines_in_batch = chunk_line_offset + 1;
            let total_pixels = pixel_count * total_lines_in_batch;
            let start_line = line_index - chunk_line_offset;

            let colors_iter = self.chunk_buffer[0..total_pixels]
                .iter()
                .map(|&raw| embedded_graphics::pixelcolor::raw::RawU16::new(raw).into());

            // Set the window constraints once for the entire block of rows
            self.display
                .set_pixels(
                    range.start as u16,
                    start_line as u16,
                    (range.end - 1) as u16,
                    line_index as u16,
                    colors_iter,
                )
                .ok();
        }
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
            .GPIO10
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO10"))?,
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
        .with_frequency(Rate::from_mhz(80))
        .with_mode(Mode::_0);

    let spi = Spi::new(
        (&mut controller.peripherals.SPI2)
            .take()
            .ok_or(CoreError::PeripheralTaken("SPI2"))?,
        spi_config,
    )? // Standard esp-hal result unpacker
    .with_sck(
        (&mut controller.peripherals.GPIO12)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO12"))?,
    )
    .with_mosi(
        (&mut controller.peripherals.GPIO11)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO11"))?,
    )
    .with_miso(
        (&mut controller.peripherals.GPIO38)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO38"))?,
    );

    let spi_device = ExclusiveDevice::new(spi, tft_cs, device_delay)?;

    static DI_BUFFER: StaticCell<[u8; 512]> = StaticCell::new();
    let buffer_ref = DI_BUFFER.init([0u8; 512]);

    let di = SpiInterface::new(spi_device, tft_dc, buffer_ref);

    let mut display = Builder::new(ST7796, di)
        .reset_pin(tft_rst)
        .color_order(ColorOrder::Bgr)
        .display_size(DISPLAY_WIDTH, DISPLAY_HEIGHT)
        .init(&mut init_delay)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    display
        .set_orientation(Orientation::new().flip_horizontal().rotate(Rotation::Deg90))
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
    screen_on: bool,
    chunk_buffer: [u16; UI_WIDTH as usize * 16],
}

impl DisplayManager {
    pub(crate) async fn spawn(
        controller: &mut Controller,
        window: Rc<MinimalSoftwareWindow>,
    ) -> Result<()> {
        let (mut display, mut tft_bl) = init_display(controller).await?;

        let mgr = DisplayManager {
            display,
            tft_bl,
            window,
            screen_on: false,
            chunk_buffer: [0u16; UI_WIDTH as usize * 16],
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
            }
        }
    }

    async fn draw(&mut self) -> Result<()> {
        // Pump Slint's clock ticks so animations and property state systems update
        slint::platform::update_timers_and_animations();

        let mut ui_did_render = false;

        self.window.draw_if_needed(|renderer| {
            ui_did_render = true;
            let start = Instant::now();

            // Pipe Slint's line updates straight over the display hardware
            renderer.render_by_line(SyncLineBuffer {
                display: &mut self.display,
                chunk_buffer: &mut self.chunk_buffer,
            });

            log::info!(
                "DisplayManager: Rendered + Flushed: {}ms",
                start.elapsed().as_millis()
            );
        });

        if ui_did_render {
            if !self.screen_on {
                // Turn on screen
                self.tft_bl
                    .set_duty(50)
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;
                self.screen_on = true;
            }
        } else {
            Timer::after(Duration::from_millis(16)).await;
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

            log::warn!("TOUCH: {:?}", data.points[0]);

            if pressed {
                // Extract raw integers from your FT6336U scan payload
                let raw_x = data.points[0].x as i32;
                let raw_y = data.points[0].y as i32;

                // Apply the landscape rotation translation matrix natively
                // This maps your portrait coordinates directly to Slint's pixel grid
                let calibrated_x = raw_y as f32;
                let calibrated_y = (320 - raw_x) as f32;

                //if self.last_x == calibrated_x && self.last_y == calibrated_y {
                // Skip dupe frames
                //    return Ok(());
                //}

                log::debug!(
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
                log::debug!("TouchManager: Touch pressed={}", pressed);

                self.window.dispatch_event(WindowEvent::PointerReleased {
                    position: LogicalPosition::new(self.last_x, self.last_y),
                    button: PointerEventButton::Left,
                });

                self.touch_active = false;
            }
        }

        // Avoid thrashing when touching the screen
        Timer::after(Duration::from_millis(16)).await;

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
