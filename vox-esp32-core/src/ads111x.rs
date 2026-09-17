use crate::common::CoreError;
use crate::panic;
use alloc::boxed::Box;
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

pub const V5_1: f32 = 5.1;

pub const ACS758LCB_050B: ACS758Version = ACS758Version {
    sens: 40.0,
    cur_dir: CurrentDirectionality::Bidirectional,
    floor_ma: 100.0, // Cut off anything below 100Mah (technically it can't read below 250Mah).
};

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
pub enum CurrentDirectionality {
    Bidirectional,
    Unidirectional,
}

#[derive(Debug, Clone, Copy)]
pub struct ACS758Version {
    sens: f32,
    cur_dir: CurrentDirectionality,
    floor_ma: f32,
}

#[derive(Debug, Clone, Copy)]
pub enum ProbeType {
    VoltageDivider {
        r1_ohms: f32,
        r2_ohms: f32,
        zero_offset: f32,
    },
    ACS758 {
        vcc: f32,
        version: ACS758Version,
        zero_offset: f32,
    },
}

impl ProbeType {
    // Constructors
    pub fn voltage_divider(r1_ohms: f32, r2_ohms: f32, zero_offset: f32) -> Self {
        ProbeType::VoltageDivider {
            r1_ohms,
            r2_ohms,
            zero_offset,
        }
    }

    pub fn acs758(vcc: f32, version: ACS758Version, zero_offset: f32) -> Self {
        ProbeType::ACS758 {
            vcc,
            version,
            zero_offset,
        }
    }
}

impl Into<Box<dyn Probe>> for ProbeType {
    fn into(self) -> Box<dyn Probe> {
        match self {
            ProbeType::VoltageDivider {
                r1_ohms,
                r2_ohms,
                zero_offset,
            } => {
                let multiplier = if (r1_ohms > 0.0 && r2_ohms > 0.0) {
                    (r1_ohms + r2_ohms) / r2_ohms
                } else {
                    0.0
                };

                Box::new(VoltageDividerProbe::new(multiplier, zero_offset))
            }
            ProbeType::ACS758 {
                vcc,
                version,
                zero_offset,
            } => Box::new(ACS758Probe::new(vcc, version, zero_offset)),
        }
    }
}

pub enum ProbeValue {
    Volts(f32),
    MilliAmps(f32),
}

impl ProbeValue {
    pub fn value(self) -> f32 {
        match self {
            ProbeValue::Volts(val) => val,
            ProbeValue::MilliAmps(val) => val,
        }
    }
}

impl Display for ProbeValue {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            ProbeValue::Volts(v) => write!(f, "{:.3} V", v),
            ProbeValue::MilliAmps(ma) => write!(f, "{:.2} mA", ma),
        }
    }
}

pub trait Probe {
    fn calculate(&self, raw_voltage: f32) -> Result<ProbeValue>;
}

struct VoltageDividerProbe {
    multiplier: f32,
    zero_offset: f32,
}

impl VoltageDividerProbe {
    fn new(multiplier: f32, zero_offset: f32) -> Self {
        Self {
            multiplier,
            zero_offset,
        }
    }
}

impl Probe for VoltageDividerProbe {
    fn calculate(&self, raw_voltage: f32) -> Result<ProbeValue> {
        if self.multiplier > 0.0 {
            let raw_corrected = raw_voltage + self.zero_offset;
            let val = (raw_corrected * self.multiplier) + self.zero_offset;

            log::trace!(
                "VoltageDividerProbe calculate: raw={}, cor={}, mul={}, val={}",
                raw_voltage,
                raw_corrected,
                self.multiplier,
                val
            );

            Ok(ProbeValue::Volts(val))
        } else {
            Ok(ProbeValue::Volts(0.0))
        }
    }
}

struct ACS758Probe {
    vcc: f32,
    version: ACS758Version,
    zero_offset: f32,
}

impl ACS758Probe {
    fn new(vcc: f32, version: ACS758Version, zero_offset: f32) -> Self {
        Self {
            vcc,
            version,
            zero_offset,
        }
    }
}

impl Probe for ACS758Probe {
    fn calculate(&self, raw_voltage: f32) -> Result<ProbeValue> {
        let calib_voltage = raw_voltage + self.zero_offset;
        // Combined factor: 1000.0 (V to mV) * 1000.0 (A to mA)
        const VOLTS_TO_MILLIAMPS: f32 = 1_000_000.0;

        let milliamps = match self.version.cur_dir {
            CurrentDirectionality::Bidirectional => {
                let base_voltage = self.vcc * 0.5;
                ((calib_voltage - base_voltage) / self.version.sens) * VOLTS_TO_MILLIAMPS
            }
            CurrentDirectionality::Unidirectional => {
                // The sensor reads from around 0.6V (for 5V).
                let base_voltage = self.vcc * 0.12;
                ((calib_voltage - base_voltage) / self.version.sens) * VOLTS_TO_MILLIAMPS
            }
        };

        Ok(ProbeValue::MilliAmps(
            if milliamps.abs() < self.version.floor_ma {
                0.0
            } else {
                milliamps
            },
        ))
    }
}

pub struct ADSConfig {
    address: Address,
    mux: InputMultiplexer,
    probe: Box<dyn Probe>,
}

impl ADSConfig {
    pub fn new(address: Address, mux: InputMultiplexer, probe_type: ProbeType) -> Self {
        Self {
            address,
            mux,
            probe: probe_type.into(),
        }
    }
}

pub struct ADSMultiProbe<K>
where
    K: Ord + Send + Debug + 'static,
{
    adc: ADS111x<I2c<'static, Async>>,
    configs: BTreeMap<K, ADSConfig>,
}

impl<K> ADSMultiProbe<K>
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
            .pga(embedded_ads111x::ProgramableGainAmplifier::V6_144);

        let adc = ADS111x::new(i2c_bus, Address::GND.byte(), config)
            .map_err(|e| anyhow!("Error creating ADS111x: {:?}", e))?;

        Ok(Self::new(adc))
    }

    pub fn cfg_probe(
        &mut self,
        key: K,
        address: Address,
        mux: InputMultiplexer,
        probe_type: ProbeType,
    ) -> Result<&mut Self> {
        let config = ADSConfig::new(address, mux, probe_type);

        self.configs.insert(key, config);

        Ok(self)
    }

    pub async fn read(&mut self, key: K) -> Result<(f32, Result<ProbeValue>)> {
        if let Some(config) = self.configs.get_mut(&key) {
            let raw_voltage = self
                .adc
                .read_single_voltage(Some(config.address.byte()), Some(config.mux))
                .await?;

            Ok((raw_voltage, config.probe.calculate(raw_voltage)))
        } else {
            Err(anyhow!("No config found for probe key: {:?}", key))
        }
    }
}
