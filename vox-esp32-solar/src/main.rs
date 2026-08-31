use anyhow::Result;
use embassy_time::Duration;
use embedded_io_async::{Read, Write};
use esp_idf_hal::gpio;
use esp_idf_hal::uart::*;
use esp_idf_hal::units::Hertz;
use rmodbus::{client::ModbusRequest, guess_response_frame_len, ModbusProto};

use vox_esp32_core::esp32_led::{LEDColor, LEDStatus};

fn main() -> Result<()> {
    let controller = vox_esp32_core::init()?;

    let config = config::Config::new().baudrate(Hertz(9600));

    // Route UART1 through your existing physical wiring (Pins 17 and 18)
    let mut uart1 = AsyncUartDriver::new(
        controller.peripherals.uart1.unwrap(), // Correct Hardware Serial Instance for S3 (UART1)
        controller.peripherals.pins.gpio17.unwrap(), // TX
        controller.peripherals.pins.gpio16.unwrap(), // RX
        Option::<gpio::Gpio0>::None,           // Empty CTS
        Option::<gpio::Gpio0>::None,           // Empty RTS
        &config,
    )?;

    let main_future = Box::pin(async {
        controller
            .led
            .set(Some(LEDColor::Teal), Some(LEDStatus::Blink), None, None)
            .await;

        let mut request_buffer: Vec<u8> = Vec::new();
        let mut mreq = ModbusRequest::new(0x01, ModbusProto::Rtu);

        loop {
            log::info!("Checking 0x01 ...");

            // Clear the vector buffer so old requests are wiped out before we build a new one
            request_buffer.clear();

            // REAL METHOD NAME: generate_get_holdings (Address, Count, Destination Vector)
            if mreq
                .generate_get_holdings(0x0100, 8, &mut request_buffer)
                .is_ok()
            {
                // Write the compiled frame out over your exact AsyncUartDriver handle
                if uart1.write_all(&request_buffer[..]).await.is_ok() {
                    // Calculate the expected incoming response length from the request packet
                    let expected_len =
                        guess_response_frame_len(&request_buffer[..], ModbusProto::Rtu).unwrap()
                            as usize;

                    let mut response_buffer = [0u8; 256];
                    let active_receive_window = &mut response_buffer[..expected_len];

                    // Pull the exact response stream from the wire non-blockingly
                    if uart1.read_exact(active_receive_window).await.is_ok() {
                        // REAL PARSING STEPS:
                        // Step A: Validate the response CRC and frame structure
                        if mreq.parse_ok(active_receive_window).is_ok() {
                            let mut values: Vec<u16> = Vec::new();

                            // Step B: Extract the big-endian payload into your u16 array
                            if mreq.parse_u16(active_receive_window, &mut values).is_ok() {
                                let battery_voltage = values[0] as f32 * 0.1;
                                log::info!("🔋 Battery Voltage: {:.1}V", battery_voltage);
                            }
                        }
                    }
                }
            }

            embassy_time::Timer::after(Duration::from_millis(3000)).await;
        }
    });

    edge_executor::block_on(controller.executor.run(main_future));

    Ok(())
}
