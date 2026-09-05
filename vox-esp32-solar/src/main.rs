#![no_std]

use anyhow::Result;
use embassy_time::Duration;
use embedded_io_async::{Read, Write};
use esp_idf_hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated, DirectConverter};
use esp_idf_hal::units::Hertz;

use vox_esp32_core::esp32_led::{LEDColor, LEDStatus};

use crate::powmr_mppt::MPPTManager;

// Testing
const VOLTAGE_DIVIDER_R1: f32 = 181400.0;
const VOLTAGE_DIVIDER_R1_WIRE: f32 = 0.65;
const VOLTAGE_DIVIDER_R2: f32 = 6038.0;

mod powmr_mppt;

/**

  GPIO Notes:
    GPIO 17: Serial Module - TX
    GPIO 18: Serial Module - RX

**/

fn main() -> Result<()> {
    let mut controller = vox_esp32_core::init()?;

    //let mppt_rx = MPPTManager::spawn(&mut controller)?;

    let config = AdcContConfig::new();

    let configured_pin = Attenuated::db12(controller.peripherals.pins.gpio4.take().unwrap());

    let mut adc_driver = AdcContDriver::new(
        controller.peripherals.adc1.take().unwrap(),
        &config,
        configured_pin, // Pass your target input pin
    )?;

    adc_driver.start()?;

    let converter = DirectConverter::new(attenuation::DB_12);

    let main_future = Box::pin(async {
        controller
            .led
            .set(Some(LEDColor::Teal), Some(LEDStatus::Blink), None, None)
            .await;

        let mut buffer = [AdcMeasurement::default(); 1024];

        let vd_multiplier = (VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE + VOLTAGE_DIVIDER_R2)
            / VOLTAGE_DIVIDER_R2;

        loop {
            match adc_driver.read_async(&mut buffer).await {
                Ok(count) if count > 0 => {
                    let mut sum: u64 = 0;
                    for i in 0..count {
                        sum += buffer[i].data() as u64;
                    }

                    let avg_raw = sum as f64 / count as f64;

                    let mut calibrated_mv: i32 = 0;

                    let raw_voltage = calibrated_mv as f32 / 1000.0;

                    log::info!(
                        "Samples Averaged: {}, Raw Avg: {:.1}, Raw Voltage: {:.2}V, Voltage: {:.2}V",
                        count,
                        avg_raw,
                        raw_voltage,
                        (raw_voltage as f32) * vd_multiplier
                    );
                }
                Ok(_) => {} // Empty read, continue polling
                Err(e) => {
                    log::info!("Async read error: {:?}", e);
                }
            }

            /*
            match mppt_rx.recv().await {
                Ok(Ok(state)) => {
                    log::info!("MPPT Rx: {}", state);
                }
                _ => {
                    // TODO:
                    log::info!("ERROR");
                }
            }
             */

            embassy_time::Timer::after(Duration::from_millis(1000)).await;
        }
    });

    edge_executor::block_on(controller.executor.run(main_future));

    Ok(())
}

/*
use esp_idf_hal::adc::{AdcChannelDriver, AdcDriver};
use esp_idf_hal::adc::config::Config;
use esp_idf_hal::gpio::ADCPin;

/// High-efficiency std utility to calculate upstream voltages via a physical resistor divider.
pub struct ResistorDivider<'a, const A: esp_idf_hal::adc::attenuation_t, PIN>
    where
        PIN: ADCPin,
{
    adc: &'a mut AdcDriver<'a, esp_idf_hal::adc::ADC1>,
    pin: AdcChannelDriver<'a, A, PIN>,
    r1_ohms: f32, // Resistor connected to Upstream/Source Voltage (e.g., 181000.0)
r2_ohms: f32, // Resistor connected to Ground (e.g., 6048.0)
}

impl<'a, PIN> ResistorDivider<'a, { esp_idf_hal::sys::adc_atten_t_ADC_ATTEN_DB_12 }, PIN>
    where
        PIN: ADCPin,
// Note: In older esp-idf-hal versions, 11dB attenuation is used via Atten11dB
// or the underlying driver types matching your specific ESP32 target.
{
    pub fn new(
        adc: &'a mut AdcDriver<'a, esp_idf_hal::adc::ADC1>,
        raw_pin: PIN,
        r1_ohms: f32,
        r2_ohms: f32
    ) -> Result<Self, esp_idf_hal::sys::EspError> {

        // Instantiate the pin driver with 11dB/12dB attenuation to scale up to ~3.3V
        let pin = AdcChannelDriver::<{ esp_idf_hal::sys::adc_atten_t_ADC_ATTEN_DB_12 }, _>::new(raw_pin)?;

        Ok(Self {
            adc,
            pin,
            r1_ohms,
            r2_ohms,
        }
    }

    /// Samples the pin synchronously via ESP-IDF and calculates the true upstream voltage
    pub fn read_upstream_voltage(&mut self) -> Result<f32, esp_idf_hal::sys::EspError> {
        // esp_idf_hal uniquely returns calibrated output in millivolts directly!
        let pin_mv: u16 = self.adc.read(&mut self.pin)?;

        // Convert millivolts integer to standard volts float
        let pin_voltage = pin_mv as f32 / 1000.0;

        // Apply inverse Voltage Divider Math to calculate upstream target:
        // Vin = Vout * ((R1 + R2) / R2)
        let upstream_voltage = pin_voltage * ((self.r1_ohms + self.r2_ohms) / self.r2_ohms);

        Ok(upstream_voltage)
    }
}

 */
