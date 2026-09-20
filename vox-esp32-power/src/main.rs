#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]
extern crate alloc;

use crate::metrics::MetricsManager;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::analog::adc::{AdcCalScheme, AdcHasCurveCal};
use vox_esp32_core::esp32_led::{LEDColor, LEDStatus};
use vox_esp32_core::types::{Device, DeviceType};

mod metrics;
mod powmr_mppt;

/**

GPIO Notes:
    GPIO 4:  ADS1115 - SCL
    GPIO 5:  ADS1115 - SDA
    GPIO 17: Serial Module - TX
    GPIO 18: Serial Module - RX

ADS1115 Notes:

 GND:

   A0: Pv1 V
   A1: Pv1 A
   A2: Pv2 V
   A3: Pv2 A

 VCC:

   A0: Pv3 V
   A1: Pv3 A
   A2: Batt V
   A3: Batt A

**/

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let mut controller = vox_esp32_core::init(Device::new(DeviceType::Power, 1), spawner)
        .expect("failed to initialize ESP controller");

    let metrics_rx =
        MetricsManager::spawn(&mut controller).expect("failed to spawn Power MetricsManager");

    controller
        .led
        .set(Some(LEDColor::Teal), Some(LEDStatus::Blink), None, None)
        .await;

    loop {
        match metrics_rx.recv().await {
            Ok(metrics) => {
                log::info!("Power Metrics: {:?}", metrics);
            }
            Err(e) => log::error!("Power Metrics Recv Error: {}", e),
        }

        Timer::after(Duration::from_millis(1000)).await;
    }
}
