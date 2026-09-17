use anyhow::Result;
use embassy_time::{Duration, Timer};
use embedded_ads111x::InputMultiplexer;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use vox_esp32_core::ads111x::{ADSMultiProbe, Address, ProbeType, ACS758LCB_050B, V5_1};
use vox_esp32_core::common::CoreError;
use vox_esp32_core::Controller;

const VOLTAGE_DIVIDER_R1: f32 = 181400.0;
const VOLTAGE_DIVIDER_R1_WIRE: f32 = 0.65;
const VOLTAGE_DIVIDER_R2: f32 = 6038.0;

#[derive(Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Debug, EnumIter)]
enum TestProbeId {
    SolarPvV1,
    SolarPvA1,
}

pub async fn run(controller: &mut Controller) -> Result<()> {
    log::info!("Started ADS1115 Voltage test ...");

    let sda = (&mut controller.peripherals.GPIO5)
        .take()
        .ok_or(CoreError::PeripheralTaken("GPIO5"))?;
    let scl = (&mut controller.peripherals.GPIO4)
        .take()
        .ok_or(CoreError::PeripheralTaken("GPIO4"))?;

    let mut ads = ADSMultiProbe::from_gpio(
        (&mut controller.peripherals.I2C0)
            .take()
            .ok_or(CoreError::PeripheralTaken("I2C0"))?,
        sda,
        scl,
    )?;

    ads.cfg_probe(
        TestProbeId::SolarPvV1,
        Address::GND,
        InputMultiplexer::AIN0GND,
        ProbeType::voltage_divider(
            VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
            VOLTAGE_DIVIDER_R2,
            0.005,
        ),
    )?
    .cfg_probe(
        TestProbeId::SolarPvA1,
        Address::GND,
        InputMultiplexer::AIN3GND,
        ProbeType::acs758(V5_1, ACS758LCB_050B, 0.00321),
    )?;

    loop {
        for probe in TestProbeId::iter() {
            match ads.read(probe).await {
                Ok((raw_voltage, Ok(probe_value))) => {
                    log::info!(
                        "[{:?}] Value: {} (raw: {:.10} V)",
                        probe,
                        probe_value,
                        raw_voltage
                    );
                }
                Ok((raw_voltage, Err(e))) => {
                    log::error!(
                        "Failed to convert voltage '{:.3} V' from '{:?}' voltage probe: {:?}",
                        raw_voltage,
                        probe,
                        e
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
