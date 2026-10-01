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
use anyhow::Result;
use async_channel::Receiver;
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
use ft6336u_dd::{Ft6336uAsync, Ft6336uInterface, TouchData};
use mipidsi::{
    interface::SpiInterface,
    models::ST7796,
    options::{ColorOrder, Orientation, Rotation},
    Builder,
};
use vox_esp32_core::async_channel::{lossy_bounded, LossyChannel};

use core::ops::Range;
use embassy_executor::Spawner;
use esp_hal::ledc::channel::Channel;
use esp_hal::peripherals::{
    GPIO10, GPIO11, GPIO12, GPIO19, GPIO2, GPIO20, GPIO21, GPIO38, GPIO39, GPIO42, GPIO47, I2C0,
    LEDC, SPI2,
};
use futures::future::{self};
use futures::pin_mut;
use futures::select_biased;
use futures::FutureExt;
use slint::platform::{
    software_renderer::{LineBufferProvider, MinimalSoftwareWindow, Rgb565Pixel},
    PointerEventButton, WindowEvent,
};
use slint::LogicalPosition;
use static_cell::StaticCell;

use vox_esp32_core::common::CoreError;
use vox_esp32_core::Controller;

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
pub const UI_WIDTH: u16 = DISPLAY_HEIGHT;
pub const UI_HEIGHT: u16 = DISPLAY_WIDTH;

pub async fn spawn(
    spawner: &Spawner,
    window: Rc<MinimalSoftwareWindow>,
    lcd_peripherals: DisplayLCDPeripherals,
    touch_peripherals: DisplayTouchPeripherals,
    init_brightness: u8,
) -> Result<(LossyChannel<u8>, Receiver<TouchData>)> {
    let brightness_tx =
        DisplayManager::spawn(spawner, window.clone(), lcd_peripherals, init_brightness).await?;
    let touched_rx = TouchManager::spawn(spawner, window, touch_peripherals).await?;

    Ok((brightness_tx, touched_rx))
}

pub struct DisplayLCDPeripherals {
    tft_cs: GPIO10<'static>,
    tft_rst: GPIO2<'static>,
    tft_dc: GPIO42<'static>,
    bl_pin: GPIO39<'static>,
    ledc: LEDC<'static>,
    spi: SPI2<'static>,
    spi_sck: GPIO12<'static>,
    spi_mosi: GPIO11<'static>,
    spi_miso: GPIO38<'static>,
}

impl DisplayLCDPeripherals {
    pub fn new(controller: &mut Controller) -> Result<Self> {
        let tft_cs = controller
            .peripherals
            .GPIO10
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO10"))?;
        let tft_rst = controller
            .peripherals
            .GPIO2
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO2"))?;
        let tft_dc = controller
            .peripherals
            .GPIO42
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO42"))?;
        let bl_pin = controller
            .peripherals
            .GPIO39
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO39"))?;
        let ledc = controller
            .peripherals
            .LEDC
            .take()
            .ok_or(CoreError::PeripheralTaken("LEDC"))?;
        let spi = controller
            .peripherals
            .SPI2
            .take()
            .ok_or(CoreError::PeripheralTaken("SPI2"))?;
        let spi_sck = controller
            .peripherals
            .GPIO12
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO12"))?;
        let spi_mosi = controller
            .peripherals
            .GPIO11
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO11"))?;
        let spi_miso = controller
            .peripherals
            .GPIO38
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO38"))?;

        Ok(Self {
            tft_cs,
            tft_rst,
            tft_dc,
            bl_pin,
            ledc,
            spi,
            spi_sck,
            spi_mosi,
            spi_miso,
        })
    }
}

pub struct DisplayTouchPeripherals {
    ctp_rst: GPIO21<'static>,
    i2c: I2C0<'static>,
    i2c_scl: GPIO47<'static>,
    i2c_sda: GPIO20<'static>,
    touch_int: GPIO19<'static>,
}

impl DisplayTouchPeripherals {
    pub fn new(controller: &mut Controller) -> Result<Self> {
        let ctp_rst = controller
            .peripherals
            .GPIO21
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO21"))?;
        let i2c = controller
            .peripherals
            .I2C0
            .take()
            .ok_or(CoreError::PeripheralTaken("I2C0"))?;
        let i2c_scl = controller
            .peripherals
            .GPIO47
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO47"))?;
        let i2c_sda = controller
            .peripherals
            .GPIO20
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO20"))?;
        let touch_int = controller
            .peripherals
            .GPIO19
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO19"))?;

        Ok(Self {
            ctp_rst,
            i2c,
            i2c_scl,
            i2c_sda,
            touch_int,
        })
    }
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
    brightness_rx: Receiver<u8>,
    init_brightness: u8,
}

impl DisplayManager {
    pub(crate) async fn spawn(
        spawner: &Spawner,
        window: Rc<MinimalSoftwareWindow>,
        peripherals: DisplayLCDPeripherals,
        init_brightness: u8,
    ) -> Result<LossyChannel<u8>> {
        let (display, tft_bl) = init_display(peripherals)?;

        let (brightness_tx, brightness_rx) = lossy_bounded(1);

        let mgr = DisplayManager {
            display,
            tft_bl,
            window,
            screen_on: false,
            chunk_buffer: [0u16; UI_WIDTH as usize * 16],
            brightness_rx,
            init_brightness,
        };

        spawner.spawn(display_task(mgr)?);

        Ok(brightness_tx)
    }

    async fn run(&mut self) {
        log::info!("DisplayManager: Started");

        let mut ui_did_render = false;
        loop {
            let brightness_rx_fut = self.brightness_rx.recv().fuse();

            let draw_timeout_fut = if ui_did_render {
                Timer::after(Duration::from_millis(1))
            } else {
                Timer::after(Duration::from_millis(16))
            }
            .fuse();

            pin_mut!(brightness_rx_fut, draw_timeout_fut);

            select_biased! {
                msg_res = brightness_rx_fut => {
                    match msg_res {
                        Ok(brightness) => {
                            if let Err(e) = self.set_brightness(brightness).await {
                                log::warn!("Failed to set brightness: {:?}", e);
                            }
                        }
                        Err(e) => {
                            log::warn!("Receive error while trying to get a set brightness msg: {:?}", e);
                        }
                    }
                }
                _ = draw_timeout_fut => {
                    match self.draw().await {
                        Ok(res) => ui_did_render = res,
                        Err(e) => {
                            log::error!("DisplayManager draw failed: {:?}", e);
                            Timer::after(Duration::from_millis(50)).await;
                            ui_did_render = false;
                        }
                    }
                }
            }
        }
    }

    async fn draw(&mut self) -> Result<bool> {
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
                self.set_brightness(self.init_brightness).await?;
                self.screen_on = true;
            }
        } else {
            Timer::after(Duration::from_millis(16)).await;
        }

        Ok(ui_did_render)
    }

    async fn set_brightness(&self, brightness: u8) -> Result<()> {
        self.tft_bl
            .set_duty(brightness)
            .map_err(|e| anyhow::anyhow!("{:?}", e))
    }
}

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

        let ptr = self.chunk_buffer.as_mut_ptr() as *mut Rgb565Pixel;
        let pool = unsafe { core::slice::from_raw_parts_mut(ptr.add(start_idx), pixel_count) };

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

fn init_display(
    peripherals: DisplayLCDPeripherals,
) -> Result<(DisplayAdapter, Channel<'static, LowSpeed>)> {
    let device_delay = Delay;
    let mut init_delay = Delay;

    // 1. Set up standard outputs for your display control lines
    let tft_cs = Output::new(peripherals.tft_cs, Level::High, OutputConfig::default());
    let tft_rst = Output::new(peripherals.tft_rst, Level::High, OutputConfig::default());
    let tft_dc = Output::new(peripherals.tft_dc, Level::Low, OutputConfig::default());
    let bl_pin = peripherals.bl_pin.degrade();

    let mut ledc = Ledc::new(peripherals.ledc);
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

    let spi = Spi::new(peripherals.spi, spi_config)? // Standard esp-hal result unpacker
        .with_sck(peripherals.spi_sck)
        .with_mosi(peripherals.spi_mosi)
        .with_miso(peripherals.spi_miso);

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
    touched_tx: LossyChannel<TouchData>,
}

impl TouchManager {
    pub(crate) async fn spawn(
        spawner: &Spawner,
        window: Rc<MinimalSoftwareWindow>,
        peripherals: DisplayTouchPeripherals,
    ) -> Result<Receiver<TouchData>> {
        let (touch, touch_int) = init_touch(peripherals).await?;

        let (touched_tx, touched_rx) = lossy_bounded(1);

        let mgr = TouchManager {
            touch,
            touch_int,
            window,
            touch_active: false,
            last_x: 0.0,
            last_y: 0.0,
            touched_tx,
        };

        spawner.spawn(touch_task(mgr)?);

        Ok(touched_rx)
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
        if !self.touch_active {
            self.touch_int.wait_for_low().await;
        }

        if let Ok(data) = self.touch.scan().await {
            let pressed = data.touch_count > 0;

            if pressed {
                // Fire touched event
                if let Err(e) = self.touched_tx.send_lossy(data).await {
                    log::warn!("Failed to send touch event: {:?}", e);
                }

                // Extract raw integers from your FT6336U scan payload
                let raw_x = data.points[0].x as i32;
                let raw_y = data.points[0].y as i32;

                // Apply the landscape rotation translation matrix natively
                // This maps your portrait coordinates directly to Slint's pixel grid
                let calibrated_x = raw_y as f32;
                let calibrated_y = (320 - raw_x) as f32;

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

        // Avoid thrashing when touching the screen
        Timer::after(Duration::from_millis(16)).await;

        Ok(())
    }
}

async fn init_touch(
    peripherals: DisplayTouchPeripherals,
) -> Result<(TouchAdapter, Input<'static>)> {
    let mut ctp_rst = Output::new(peripherals.ctp_rst, Level::Low, OutputConfig::default());
    Timer::after(Duration::from_millis(10)).await;
    ctp_rst.set_high(); // Release reset line
    Timer::after(Duration::from_millis(50)).await;

    let i2c_config = I2cConfig::default().with_frequency(Rate::from_khz(400)); // Standard Fast-Mode I2C

    let i2c = I2c::new(peripherals.i2c, i2c_config)?
        .with_scl(peripherals.i2c_scl) // black
        .with_sda(peripherals.i2c_sda) // red
        .into_async();

    let touch_int = Input::new(
        peripherals.touch_int,
        esp_hal::gpio::InputConfig::default().with_pull(Pull::Up),
    );

    Ok((Ft6336uAsync::new(i2c), touch_int))
}
