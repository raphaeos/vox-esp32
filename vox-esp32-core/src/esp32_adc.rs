use esp_hal::analog::adc;
use esp_hal::analog::adc::{Adc, AdcCalScheme, AdcPin};
use esp_hal::Async;

pub struct Esp32VoltageProbe {
    r1_ohms: f32,
    r2_ohms: f32,
    vd_multiplier: f32,
}

impl Esp32VoltageProbe {
    pub fn new(r1_ohms: f32, r2_ohms: f32) -> Self {
        let vd_multiplier = (r1_ohms + r2_ohms) / r2_ohms;

        Self {
            r1_ohms,
            r2_ohms,
            vd_multiplier,
        }
    }

    pub async fn read<P, A, CS>(
        &self,
        adc: &mut Adc<'_, A, Async>,
        adc_pin: &mut AdcPin<P, A, CS>,
    ) -> (u16, f32, f32)
    where
        A: adc::RegisterAccess + 'static + adc::Instance,
        P: esp_hal::gpio::Pin + adc::AdcChannel + esp_hal::gpio::AnalogPin,
        CS: AdcCalScheme<A>,
    {
        let raw_reading = adc.read_oneshot(adc_pin).await;

        let raw_voltage = raw_reading as f32 / 1000.0;

        (raw_reading, raw_voltage, raw_voltage * self.vd_multiplier)
    }
}
