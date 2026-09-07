use embassy_time::{Duration, Timer};
use esp_hal::analog::adc::{Adc, AdcCalCurve, AdcConfig, Attenuation};
use esp_hal::peripherals::ADC1;
use vox_esp32_core::common::CoreError;
use vox_esp32_core::esp32_adc::Esp32VoltageProbe;
use vox_esp32_core::Controller;

const VOLTAGE_DIVIDER_R1: f32 = 181400.0;
const VOLTAGE_DIVIDER_R1_WIRE: f32 = 0.65;
const VOLTAGE_DIVIDER_R2: f32 = 6038.0;

pub async fn run(controller: &mut Controller) -> ! {
    log::info!("Started ADC Voltage test ...");

    let mut adc_config = AdcConfig::new();

    let mut adc_pin = adc_config.enable_pin_with_cal::<_, AdcCalCurve<ADC1>>(
        (&mut controller.peripherals.GPIO4).take().unwrap(),
        Attenuation::_11dB,
    );

    let mut adc = Adc::new(
        controller
            .peripherals
            .ADC1
            .take()
            .ok_or(CoreError::PeripheralTaken("ADC1"))
            .unwrap(),
        adc_config,
    )
    .into_async();

    let adc_vol_probe = Esp32VoltageProbe::new(
        VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
        VOLTAGE_DIVIDER_R2,
    );

    loop {
        let (raw_value, raw_voltage, voltage) = adc_vol_probe.read(&mut adc, &mut adc_pin).await;

        log::info!(
            "Reading[raw: {}, raw_voltage: {:.2} V, voltage, {:.2} V]",
            raw_value,
            raw_voltage,
            voltage
        );

        Timer::after(Duration::from_millis(1000)).await;
    }
}
