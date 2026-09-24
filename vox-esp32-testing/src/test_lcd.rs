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
};
use ft6336u_dd::Ft6336uAsync;
use lcd_async::interface::SpiInterface;
use lcd_async::models::ST7796;
use lcd_async::Builder;
use num_traits::float::FloatCore;
use vox_esp32_core::common::CoreError;
use vox_esp32_core::Controller;

pub async fn run(controller: &mut Controller) -> anyhow::Result<()> {
    log::info!("Started LCD test ...");

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
        .display_size(320, 480)
        .init(&mut init_delay)
        .await
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

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

    let mut touch_int = Input::new(
        (&mut controller.peripherals.GPIO19)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO19"))?,
        esp_hal::gpio::InputConfig::default().with_pull(Pull::Up),
    );

    let mut touch = Ft6336uAsync::new(i2c);

    // Draw
    const WIDTH: u16 = 320;
    const HEIGHT: u16 = 480;
    let mut row_buffer = [0u8; 320 * 2];

    // Loop through each horizontal row index
    for row_idx in 0..HEIGHT {
        // 1. Calculate a linear spectrum value skipping the Red segment.
        // Maps row 0..480 to a fractional value between 0.0 and 1.0
        let progress = row_idx as f32 / HEIGHT as f32;

        // 2. Generate a standard HSV-to-RGB565 spectrum point
        // We restrict the hue domain to stay between 60.0 (Yellow) and 300.0 (Violet)
        let hue = 60.0 + (progress * 240.0);
        let rgb = hsv_to_rgb565(hue, 1.0, 1.0);

        let high_byte = (rgb >> 8) as u8;
        let low_byte = (rgb & 0xFF) as u8;

        // 3. Fill the entire row buffer with this specific color point
        for chunk in row_buffer.chunks_exact_mut(2) {
            chunk[0] = high_byte;
            chunk[1] = low_byte;
        }

        // 4. Blast the single-color line down to the display matrix via DMA
        display
            .show_raw_data(0, row_idx, WIDTH, 1, &mut row_buffer)
            .await
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
    }

    // Turn on screen
    tft_bl.set_high();

    loop {
        log::info!("Waiting for touch events ...");

        touch_int.wait_for_low().await;

        // scan() reads both potential points in a single async operation
        if let Ok(touch_data) = touch.scan().await {
            log::info!(
                "[count: {}] Touch 1 -> X: {}, Y: {}, Status: {:?}, Touch 2 -> X: {}, Y: {}, Status: {:?}",
                touch_data.touch_count,
                touch_data.points[0].x,
                touch_data.points[0].y,
                touch_data.points[0].status,
                touch_data.points[1].x,
                touch_data.points[1].y,
                touch_data.points[1].status
            );
        }

        Timer::after(Duration::from_millis(10)).await;
    }
}

fn hsv_to_rgb565(h: f32, s: f32, v: f32) -> u16 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 120.0 {
        (c, x, 0.0) // Yellow -> Green
    } else if h < 180.0 {
        (x, c, 0.0) // Green -> Cyan
    } else if h < 240.0 {
        (0.0, c, x) // Cyan -> Blue
    } else {
        (0.0, x, c) // Blue -> Violet
    };

    // Compress values into 5-6-5 standard bit layout spaces
    let r_pixel = (((r + m) * 31.0).round() as u16) << 11;
    let g_pixel = (((g + m) * 63.0).round() as u16) << 5;
    let b_pixel = ((b + m) * 31.0).round() as u16;

    r_pixel | g_pixel | b_pixel
}
