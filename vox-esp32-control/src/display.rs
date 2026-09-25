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
use esp_hal::gpio::{Input, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    spi::{
        master::{Config, Spi},
        Mode,
    },
    Async,
};
use ft6336u_dd::{Ft6336u, Ft6336uAsync, Ft6336uInterface};
use lcd_async::interface::SpiInterface;
use lcd_async::models::ST7796;
use lcd_async::{Builder, Display};
use num_traits::float::FloatCore;
use vox_esp32_core::common::CoreError;
use vox_esp32_core::Controller;

use crate::ui::AppWindow;
use core::ops::Range;
use lcd_async::options::{Orientation, Rotation};
use slint::platform::{
    software_renderer::{
        LineBufferProvider, MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel,
    },
    PointerEventButton, WindowEvent,
};
use slint::LogicalPosition;

pub type DisplayAdapter = Display<
    SpiInterface<ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>, Output<'static>>,
    ST7796,
    Output<'static>,
>;
pub type TouchAdapter =
    Ft6336uAsync<Ft6336uInterface<I2c<'static, Async>>, esp_hal::i2c::master::Error>;

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

async fn init_display(controller: &mut Controller) -> Result<(DisplayAdapter, Output<'static>)> {
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
    let mut tft_bl = Output::new(
        controller
            .peripherals
            .GPIO39
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO39"))?,
        Level::Low,
        OutputConfig::default(),
    ); // Backlight ON

    let spi_config = Config::default()
        .with_frequency(esp_hal::time::Rate::from_mhz(40))
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

    let i2c_config = I2cConfig::default().with_frequency(esp_hal::time::Rate::from_khz(400)); // Standard Fast-Mode I2C

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

pub async fn run(
    controller: &mut Controller,
    window: Rc<MinimalSoftwareWindow>,
    app: AppWindow,
) -> Result<()> {
    log::info!("Started LCD ...");

    let (mut display, mut tft_bl) = init_display(controller).await?;
    let (mut touch, mut touch_int) = init_touch(controller).await?;

    let mut render_buffer = Vec::with_capacity(UI_WIDTH as usize * UI_HEIGHT as usize);
    render_buffer.resize(UI_WIDTH as usize * UI_HEIGHT as usize, Rgb565Pixel(0));

    let mut transfer_buffer = Vec::with_capacity(UI_WIDTH as usize * UI_HEIGHT as usize * 2); // 2 bytes per pixel
    transfer_buffer.resize(UI_WIDTH as usize * UI_HEIGHT as usize * 2, 0u8);

    let mut screen_on = false;
    let mut touch_active = false;
    let mut last_x = 0.0;
    let mut last_y = 0.0;

    loop {
        // Feed the watchdog.
        controller.feed();

        // Pump Slint's clock ticks so animations and property state systems update
        slint::platform::update_timers_and_animations();

        // 1. Immediate-mode check: only touch the frame memory if Slint actively redraws it
        let mut ui_did_render = false;
        window.draw_if_needed(|renderer| {
            renderer.render_by_line(Framebuffer {
                pixels: render_buffer.as_mut_slice(),
            });
            ui_did_render = true;
        });

        if ui_did_render {
            // Upload the framebuffer asynchronously.
            //
            // Rgb565Pixel is a 16-bit RGB565 pixel. This converts the framebuffer
            // to the byte slice expected by lcd_async.
            let raw_render_bytes = unsafe {
                core::slice::from_raw_parts(
                    render_buffer.as_ptr() as *const u8,
                    render_buffer.len() * size_of::<Rgb565Pixel>(),
                )
            };

            // APPLY THE HARDWARE OVERRIDES DIRECTLY TO THE BYTE STREAM
            // This splits the 16-bit word cleanly into individual byte slots,
            // correcting the ST7796S controller's native color inversion and channel mismatch.
            for (i, chunk) in raw_render_bytes.chunks_exact(2).enumerate() {
                let low_byte = chunk[0];
                let high_byte = chunk[1];

                // Use your working byte-level transformations directly
                transfer_buffer[i * 2] = !high_byte;
                transfer_buffer[i * 2 + 1] = !low_byte;
            }

            display
                .show_raw_data(0, 0, UI_WIDTH, UI_HEIGHT, transfer_buffer.as_slice())
                .await
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;

            if !screen_on {
                // Turn on screen
                tft_bl.set_high();
                screen_on = true;
            }
        }

        // Touch interrupt.
        if touch_int.is_low() {
            if let Ok(data) = touch.scan().await {
                let pressed = data.touch_count > 0;

                if pressed {
                    let x = data.points[0].x as f32;
                    let y = data.points[0].y as f32;

                    if !touch_active {
                        window.dispatch_event(WindowEvent::PointerPressed {
                            position: LogicalPosition::new(x, y),
                            button: PointerEventButton::Left,
                        });

                        touch_active = true;
                    } else {
                        window.dispatch_event(WindowEvent::PointerMoved {
                            position: LogicalPosition::new(x, y),
                        });
                    }

                    last_x = x;
                    last_y = y;
                } else if touch_active {
                    window.dispatch_event(WindowEvent::PointerReleased {
                        position: LogicalPosition::new(last_x, last_y),
                        button: PointerEventButton::Left,
                    });

                    touch_active = false;
                }
            }
        }

        Timer::after(Duration::from_millis(16)).await;
    }
}
