use anyhow::{anyhow, Result};
use embassy_time::{Duration, Timer};
use embedded_ads111x::{ADS111x, ADS111xConfig, InputMultiplexer};
use esp_hal::{
    i2c::master::{Config, I2c},
    peripherals::Peripherals,
};
use strum::IntoEnumIterator; // 1. Import the trait
use strum_macros::EnumIter;
use vox_esp32_core::ads111x::{ADSVoltageProbe, Address};
use vox_esp32_core::common::CoreError;
use vox_esp32_core::esp32_adc::Esp32VoltageProbe;
use vox_esp32_core::Controller; // 2. Import the derive macro

const VOLTAGE_DIVIDER_R1: f32 = 181400.0;
const VOLTAGE_DIVIDER_R1_WIRE: f32 = 0.65;
const VOLTAGE_DIVIDER_R2: f32 = 6038.0;

#[derive(Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Debug, EnumIter)]
enum TestProbeId {
    SolarPv1,
    SolarPv2,
    SolarPv3,
}

pub async fn run(controller: &mut Controller) -> Result<()> {
    log::info!("Started ADS1115 Voltage test ...");

    let sda = (&mut controller.peripherals.GPIO5)
        .take()
        .ok_or(CoreError::PeripheralTaken("GPIO5"))?;
    let scl = (&mut controller.peripherals.GPIO4)
        .take()
        .ok_or(CoreError::PeripheralTaken("GPIO4"))?;

    let mut ads = ADSVoltageProbe::from_gpio(
        (&mut controller.peripherals.I2C0)
            .take()
            .ok_or(CoreError::PeripheralTaken("I2C0"))?,
        sda,
        scl,
    )?;
    ads.cfg_probe(
        TestProbeId::SolarPv1,
        Address::GND,
        InputMultiplexer::AIN0GND,
        VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
        VOLTAGE_DIVIDER_R2,
    )?
    .cfg_probe(
        TestProbeId::SolarPv2,
        Address::VCC,
        InputMultiplexer::AIN0GND,
        VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
        VOLTAGE_DIVIDER_R2,
    )?
    .cfg_probe(
        TestProbeId::SolarPv3,
        Address::VCC,
        InputMultiplexer::AIN1GND,
        VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
        VOLTAGE_DIVIDER_R2,
    )?;

    loop {
        for probe in TestProbeId::iter() {
            match ads.read(probe).await {
                Ok((raw_voltage, voltage)) => {
                    log::info!(
                        "[{:?}] Voltage: {:.3} V (raw: {:.3} V)",
                        probe,
                        voltage,
                        raw_voltage
                    );
                }
                Err(e) => {
                    log::error!("Failed to read '{:?}' voltage probe: {:?}", probe, e);
                }
            }
        }

        Timer::after(Duration::from_millis(1000)).await;
    }
}
