use anyhow::Result;
use embassy_time::{Duration, Timer};
use embedded_ads111x::{ADS111x, ADS111xConfig, InputMultiplexer};
use esp_hal::{
    gpio::Io,
    i2c::master::{Config, I2c},
    peripherals::Peripherals,
};
use vox_esp32_core::common::CoreError;
use vox_esp32_core::esp32_adc::Esp32VoltageProbe;
use vox_esp32_core::Controller;

const VOLTAGE_DIVIDER_R1: f32 = 181400.0;
const VOLTAGE_DIVIDER_R1_WIRE: f32 = 0.65;
const VOLTAGE_DIVIDER_R2: f32 = 6038.0;

pub async fn run(controller: &mut Controller) -> ! {
    log::info!("Started ADS1115 Voltage test ...");

    let sda = (&mut controller.peripherals.GPIO5).take().unwrap();
    let scl = (&mut controller.peripherals.GPIO4).take().unwrap();

    let i2c_bus = I2c::new(
        (&mut controller.peripherals.I2C0).take().unwrap(),
        Config::default(),
    )
    .unwrap()
    .with_sda(sda)
    .with_scl(scl)
    .into_async();

    // Configure the ADC directly using its modern config builder pattern
    let config = ADS111xConfig::default()
        .mux(InputMultiplexer::AIN0GND)
        .pga(embedded_ads111x::ProgramableGainAmplifier::V4_096);

    let mut adc = ADS111x::new(i2c_bus, 0x48u8, config).unwrap();

    /*
    let adc_vol_probe = Esp32VoltageProbe::new(
        VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
        VOLTAGE_DIVIDER_R2,
    );

     */

    loop {
        match adc.read_single_voltage(None).await {
            Ok(raw_value) => {
                log::info!("Read Voltage: {:.2} V", raw_value);
            }
            Err(_) => {
                log::error!("Async I2C reading failed!");
            }
        }

        //let (raw_value, raw_voltage, voltage) = adc_vol_probe.read(&mut adc, &mut adc_pin).await;

        /*
        log::info!(
            "Reading[raw: {}, raw_voltage: {:.2} V, voltage, {:.2} V]",
            raw_value,
            raw_voltage,
            voltage
        );

         */

        Timer::after(Duration::from_millis(1000)).await;
    }
}
