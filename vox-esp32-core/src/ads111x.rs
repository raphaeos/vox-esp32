use crate::common::CoreError;
use alloc::collections::BTreeMap;
use anyhow::{anyhow, Result};
use core::fmt::{Debug, Display};
use embedded_ads111x::{ADS111x, ADS111xConfig, InputMultiplexer};
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::i2c::master::Instance;
use esp_hal::{
    i2c::master::{Config, I2c},
    Async,
};

pub enum Address {
    GND,
    VCC,
    SDA,
    SCL,
}

impl Address {
    pub fn byte(&self) -> u8 {
        match self {
            Address::GND => 0x48u8,
            Address::VCC => 0x49u8,
            Address::SDA => 0x4au8,
            Address::SCL => 0x4Bu8,
        }
    }
}

pub struct ADSVoltageProbeConfig {
    address: Address,
    mux: InputMultiplexer,
    vd_multiplier: f32,
}

impl ADSVoltageProbeConfig {
    pub fn new(address: Address, mux: InputMultiplexer, r1_ohms: f32, r2_ohms: f32) -> Self {
        let vd_multiplier = if (r1_ohms > 0.0 && r2_ohms > 0.0) {
            (r1_ohms + r2_ohms) / r2_ohms
        } else {
            0.0
        };

        Self {
            address,
            mux,
            vd_multiplier,
        }
    }
}

pub struct ADSVoltageProbe<K>
where
    K: Ord + Send + Debug + 'static,
{
    adc: ADS111x<I2c<'static, Async>>,
    configs: BTreeMap<K, ADSVoltageProbeConfig>,
}

impl<K> ADSVoltageProbe<K>
where
    K: Ord + Send + Debug + 'static,
{
    pub fn new(adc: ADS111x<I2c<'static, Async>>) -> Self {
        Self {
            adc,
            configs: BTreeMap::new(),
        }
    }

    pub fn from_gpio(
        i2c: impl Instance + 'static,
        sda: impl PeripheralInput<'static> + PeripheralOutput<'static>,
        scl: impl PeripheralInput<'static> + PeripheralOutput<'static>,
    ) -> Result<Self> {
        let i2c_bus = I2c::new(i2c, Config::default())?
            .with_sda(sda)
            .with_scl(scl)
            .into_async();

        let config = ADS111xConfig::default()
            .mux(InputMultiplexer::AIN0GND)
            .pga(embedded_ads111x::ProgramableGainAmplifier::V4_096);

        let adc = ADS111x::new(i2c_bus, Address::GND.byte(), config)
            .map_err(|e| anyhow!("Error creating ADS111x: {:?}", e))?;

        Ok(Self::new(adc))
    }

    pub fn cfg_probe(
        &mut self,
        key: K,
        address: Address,
        mux: InputMultiplexer,
        r1_ohms: f32,
        r2_ohms: f32,
    ) -> Result<&mut Self> {
        let config = ADSVoltageProbeConfig::new(address, mux, r1_ohms, r2_ohms);

        self.configs.insert(key, config);

        Ok(self)
    }

    pub async fn read(&mut self, key: K) -> Result<(f32, f32)> {
        if let Some(config) = self.configs.get_mut(&key) {
            let raw_voltage = self
                .adc
                .read_single_voltage(Some(config.address.byte()), Some(config.mux))
                .await?;
            if config.vd_multiplier > 0.0 {
                Ok((raw_voltage, raw_voltage * config.vd_multiplier))
            } else {
                Ok((raw_voltage, 0.0))
            }
        } else {
            Err(anyhow!("No config found for probe key: {:?}", key))
        }
    }
}
