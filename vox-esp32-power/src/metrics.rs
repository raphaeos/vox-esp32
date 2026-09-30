use crate::powmr_mppt::{MPPTManager, MPPTResult, MPPTState};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use anyhow::{anyhow, Result};
use async_channel::{bounded, Receiver, Sender};
use core::fmt::Debug;
use embassy_time::{Duration, Timer};
use embedded_ads111x::InputMultiplexer;
use futures::pin_mut;
use futures::select_biased;
use futures::FutureExt;
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};
use thiserror::Error;
use vox_esp32_core::ads111x::{
    ADSMultiProbe, Address, ProbeType, ACS758LCB_050B, QNHCK1_21_300_AMPS, V5_1,
};
use vox_esp32_core::async_channel::{lossy_bounded, LossyChannel};
use vox_esp32_core::common::CoreError;
use vox_esp32_core::esp32_led::LEDManagerHandle;
use vox_esp32_core::Controller;

pub const POWER_UNIT_VOLTS: &'static str = "V";
pub const POWER_UNIT_MILLI_VOLTS: &'static str = "mV";
pub const POWER_UNIT_AMPS: &'static str = "A";
pub const POWER_UNIT_MILLI_AMPS: &'static str = "mA";
pub const POWER_UNIT_WATTS: &'static str = "W";
pub const POWER_UNIT_MILLI_WATTS: &'static str = "mW";

const VOLTAGE_DIVIDER_R1: f32 = 181400.0;
const VOLTAGE_DIVIDER_R1_WIRE: f32 = 0.65;
const VOLTAGE_DIVIDER_R2: f32 = 6038.0;

#[derive(Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Debug, EnumIter)]
enum ProbeId {
    PvV1,
    PvA1,
    PvV2,
    PvA2,
    PvV3,
    PvA3,
    BattV,
    BattA,
}

impl ProbeId {
    fn va_id(&self) -> VAId {
        match self {
            ProbeId::PvV1 => VAId::Pv1,
            ProbeId::PvA1 => VAId::Pv1,
            ProbeId::PvV2 => VAId::Pv2,
            ProbeId::PvA2 => VAId::Pv2,
            ProbeId::PvV3 => VAId::Pv3,
            ProbeId::PvA3 => VAId::Pv3,
            ProbeId::BattV => VAId::Batt,
            ProbeId::BattA => VAId::Batt,
        }
    }

    fn enabled(&self) -> bool {
        self.va_id().enabled()
    }

    fn register(&self, ads: &mut ADSMultiProbe<ProbeId>) -> Result<()> {
        match self {
            // Pv 1
            ProbeId::PvV1 => ads.cfg_probe(
                *self,
                Address::GND,
                InputMultiplexer::AIN0GND,
                ProbeType::voltage_divider(
                    VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
                    VOLTAGE_DIVIDER_R2,
                    0.005,
                ),
            )?,
            ProbeId::PvA1 => ads.cfg_probe(
                *self,
                Address::GND,
                InputMultiplexer::AIN1GND,
                ProbeType::acs758(V5_1, ACS758LCB_050B, 0.00321),
            )?,
            // Pv 2
            ProbeId::PvV2 => ads.cfg_probe(
                *self,
                Address::GND,
                InputMultiplexer::AIN2GND,
                ProbeType::voltage_divider(
                    VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
                    VOLTAGE_DIVIDER_R2,
                    0.005,
                ),
            )?,
            ProbeId::PvA2 => ads.cfg_probe(
                *self,
                Address::GND,
                InputMultiplexer::AIN3GND,
                ProbeType::acs758(V5_1, ACS758LCB_050B, 0.00321),
            )?,
            // Pv 3
            ProbeId::PvV3 => ads.cfg_probe(
                *self,
                Address::VCC,
                InputMultiplexer::AIN0GND,
                ProbeType::voltage_divider(
                    VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
                    VOLTAGE_DIVIDER_R2,
                    0.005,
                ),
            )?,
            ProbeId::PvA3 => ads.cfg_probe(
                *self,
                Address::VCC,
                InputMultiplexer::AIN1GND,
                ProbeType::acs758(V5_1, ACS758LCB_050B, 0.00321),
            )?,

            // Batt
            ProbeId::BattV => ads.cfg_probe(
                *self,
                Address::VCC,
                InputMultiplexer::AIN2GND,
                ProbeType::voltage_divider(
                    VOLTAGE_DIVIDER_R1 + VOLTAGE_DIVIDER_R1_WIRE,
                    VOLTAGE_DIVIDER_R2,
                    0.005,
                ),
            )?,
            ProbeId::BattA => ads.cfg_probe(
                *self,
                Address::VCC,
                InputMultiplexer::AIN3GND,
                ProbeType::qnhck121(2.4835, QNHCK1_21_300_AMPS),
            )?,
        };

        Ok(())
    }
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumIter, Serialize, Deserialize)]
pub enum VAType {
    Batt,
    Pv,
}

#[derive(
    Debug,
    Display,
    Copy,
    Clone,
    PartialOrd,
    PartialEq,
    Ord,
    Eq,
    Serialize,
    Deserialize,
    EnumIter,
    TryFromPrimitive,
)]
#[repr(u8)]
pub enum VAId {
    Batt = 1,
    Pv1 = 10,
    Pv2 = 11,
    Pv3 = 12,
}

impl VAId {
    pub fn va_type(&self) -> VAType {
        match self {
            VAId::Pv1 => VAType::Pv,
            VAId::Pv2 => VAType::Pv,
            VAId::Pv3 => VAType::Pv,
            VAId::Batt => VAType::Batt,
        }
    }

    pub fn enabled(&self) -> bool {
        match self {
            VAId::Pv1 => true,
            VAId::Pv2 => true,
            VAId::Pv3 => true,
            VAId::Batt => true,
        }
    }

    fn v_probe(&self) -> ProbeId {
        match self {
            VAId::Pv1 => ProbeId::PvV1,
            VAId::Pv2 => ProbeId::PvV2,
            VAId::Pv3 => ProbeId::PvV3,
            VAId::Batt => ProbeId::BattV,
        }
    }

    fn a_probe(&self) -> ProbeId {
        match self {
            VAId::Pv1 => ProbeId::PvA1,
            VAId::Pv2 => ProbeId::PvA2,
            VAId::Pv3 => ProbeId::PvA3,
            VAId::Batt => ProbeId::BattA,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub va_entries: BTreeMap<VAId, VAMetricEntry>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            va_entries: BTreeMap::new(),
        }
    }

    pub fn apply_va(&mut self, id: VAId, va: VAMetricEntry) {
        self.va_entries.insert(id, va);
    }

    fn update_va(&mut self, id: VAId, status: ProbeStatus, voltage: f32, amperage: f32) {
        self.va_entries
            .entry(id)
            .and_modify(|entry| {
                entry.status = status;
                entry.voltage = voltage;
                entry.amperage = amperage;
            })
            .or_insert_with(|| VAMetricEntry::new(id, status, voltage, amperage));
    }

    fn update_va_status(&mut self, id: VAId, status: ProbeStatus) {
        self.va_entries
            .entry(id)
            .and_modify(|entry| {
                entry.status = status;
            })
            .or_insert_with(|| VAMetricEntry::new(id, status, 0.0, 0.0));
    }

    pub fn summary(&self) -> MetricsSummary {
        self.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub types: BTreeMap<VAType, SummaryMetricEntry>,
}

impl From<&Metrics> for MetricsSummary {
    fn from(value: &Metrics) -> Self {
        let mut types: BTreeMap<VAType, SummaryMetricEntry> = BTreeMap::new();

        for (_, entry) in value.va_entries.iter() {
            if let Some(type_entry) = types.get_mut(&entry.id.va_type()) {
                type_entry.voltage += entry.voltage;
                type_entry.amperage += entry.amperage;
                type_entry.wattage += entry.wattage;

                match entry.status {
                    ProbeStatus::Init => {
                        if type_entry.status != SummaryMetricStatus::Init {
                            type_entry.status = entry.status.into();
                        }
                    },
                    ProbeStatus::Ok => {
                        match type_entry.status {
                            SummaryMetricStatus::Ok | SummaryMetricStatus::Degraded  => {}, // Do not change
                            SummaryMetricStatus::Init => type_entry.status = entry.status.into(),
                            SummaryMetricStatus::Error => type_entry.status = SummaryMetricStatus::Degraded, 
                        }
                    },
                    ProbeStatus::Err(probe_error) => {
                        match type_entry.status {
                            SummaryMetricStatus::Error | SummaryMetricStatus::Degraded  => {}, // Do not change
                            SummaryMetricStatus::Init => type_entry.status = entry.status.into(),
                            SummaryMetricStatus::Ok => type_entry.status = SummaryMetricStatus::Degraded, 
                        }
                    }
                }
            } else {
                types.insert(entry.id.va_type(), SummaryMetricEntry { 
                    r#type: entry.id.va_type(), 
                    status: entry.status.into(), 
                    voltage: entry.voltage, 
                    amperage: entry.amperage, 
                    wattage: entry.wattage
                });
            }
        }

        // Ensure we have stubs.
        for va_type in VAType::iter() {
            if !types.contains_key(&va_type) {
                types.insert(va_type, SummaryMetricEntry { 
                    r#type: va_type, 
                    status: SummaryMetricStatus::Init, 
                    voltage: 0.0, 
                    amperage: 0.0, 
                    wattage: 0.0
                });
            }
        }

        Self { types }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryMetricEntry {
    pub r#type: VAType,
    pub status: SummaryMetricStatus,
    pub voltage: f32,
    pub amperage: f32,
    pub wattage: f32
}

impl SummaryMetricEntry {
    pub fn format_voltage(&self) -> String {
        format_voltage(self.voltage)
    }

    pub fn format_amperage(&self) -> String {
        format_amperage(self.amperage)
    }

    pub fn format_wattage(&self) -> String {
        format_wattage(self.wattage)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryMetricStatus {
    Init,
    Ok,
    Degraded,
    Error
}

impl From<ProbeStatus> for SummaryMetricStatus {
    fn from(value: ProbeStatus) -> Self {
        match value {
            ProbeStatus::Init => Self::Init,
            ProbeStatus::Ok => Self::Ok,
            ProbeStatus::Err(_) => Self::Error,
        }
    }
}

#[derive(Error, Copy, Clone, Debug, Serialize, Deserialize, TryFromPrimitive)]
#[repr(u8)]
pub enum ProbeError {
    #[error("Read error")]
    ReadError = 2,
    #[error("Conversion error")]
    ConversionError = 3,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum ProbeStatus {
    Init,
    Ok,
    Err(ProbeError),
}

impl Into<u8> for ProbeStatus {
    fn into(self) -> u8 {
        match self {
            ProbeStatus::Init => 0,
            ProbeStatus::Ok => 1,
            ProbeStatus::Err(e) => e as u8,
        }
    }
}

impl TryFrom<u8> for ProbeStatus {
    type Error = CoreError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ProbeStatus::Init),
            1 => Ok(ProbeStatus::Ok),
            _ => Ok(ProbeStatus::Err(
                ProbeError::try_from(value).map_err(|e| CoreError::Other(anyhow!(e)))?,
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VAMetricEntry {
    pub id: VAId,
    pub status: ProbeStatus,
    pub voltage: f32,
    pub amperage: f32,
    pub wattage: f32
}

impl VAMetricEntry {
    fn new(id: VAId, status: ProbeStatus, voltage: f32, amperage: f32) -> Self {
        Self {
            id,
            status,
            voltage,
            amperage,
            wattage: calculate_milliwatts(voltage, amperage)
        }
    }

    pub fn from_bytes(id: VAId, bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 8 {
            return Err(anyhow!("Expected 8 bytes, got {}", bytes.len()));
        }

        let status = ProbeStatus::try_from(bytes[0]).map_err(|e| CoreError::Other(anyhow!(e)))?;

        let voltage = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], 0]);
        let amperage = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        Ok(Self {
            id,
            status,
            voltage: voltage as f32,
            amperage: amperage as f32,
            wattage: calculate_milliwatts(voltage as f32, amperage as f32)
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        const I24_MAX: i32 = 16_777_215;
        const I24_MIN: i32 = -16_777_216;

        let amperage: i32 = self.amperage as i32;
        if amperage > I24_MAX || amperage < I24_MIN {
            return Err(anyhow!(
                "Amperage overflows beyond i24 (3 bytes): {}",
                amperage
            ));
        }

        let mut bytes = alloc::vec![0u8; 8];

        bytes[0] = self.status.into();

        // Take the first 3 bytes of the millivolts (a 3-byte integer (u24) can store values up to 16,777,215)
        let v_bytes = (self.voltage as i32).to_le_bytes();
        bytes[1..4].copy_from_slice(&v_bytes[0..3]);
        bytes[4..8].copy_from_slice(&amperage.to_le_bytes());

        Ok((bytes))
    }

    pub fn format_voltage(&self) -> String {
        format_voltage(self.voltage)
    }

    pub fn format_amperage(&self) -> String {
        format_amperage(self.amperage)
    }

    pub fn format_wattage(&self) -> String {
        format_wattage(self.wattage)
    }
}

#[embassy_executor::task]
async fn metrics_task(mut mgr: MetricsManager) {
    mgr.run().await;
}

pub struct MetricsManager {
    m_tx: LossyChannel<Metrics>,
    va_tx: LossyChannel<VAMetricEntry>,
    probe: ADSMultiProbe<ProbeId>,
    led: LEDManagerHandle,
    metrics: Metrics,
}

enum ReadVAResult {
    Ok,
    ReadError,
    ConversionError,
}

impl MetricsManager {
    pub(crate) fn spawn(
        controller: &mut Controller,
    ) -> Result<(Receiver<Metrics>, Receiver<VAMetricEntry>)> {
        let sda = (&mut controller.peripherals.GPIO5)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO5"))?;
        let scl = (&mut controller.peripherals.GPIO4)
            .take()
            .ok_or(CoreError::PeripheralTaken("GPIO4"))?;

        let mut probe = ADSMultiProbe::from_gpio(
            (&mut controller.peripherals.I2C0)
                .take()
                .ok_or(CoreError::PeripheralTaken("I2C0"))?,
            sda,
            scl,
        )?;

        for probe_id in ProbeId::iter() {
            if probe_id.enabled() {
                probe_id.register(&mut probe)?;
            }
        }

        let (m_tx, m_rx) = lossy_bounded(10);
        let (va_tx, va_rx) = lossy_bounded(10);

        let led = controller.led.clone();

        let mgr = MetricsManager {
            m_tx,
            va_tx,
            probe,
            led,
            metrics: Metrics::new(),
        };

        controller.spawn(metrics_task(mgr)?);

        Ok((m_rx, va_rx))
    }

    async fn run(&mut self) {
        log::info!("Vox ESP32 Solar: Metrics Manager started");

        loop {
            let timeout_fut = Timer::after(Duration::from_millis(1000)).fuse();

            pin_mut!(timeout_fut);

            select_biased! {
                _ = timeout_fut => {
                    self.poll_probes().await;
                }
            }
        }
    }

    async fn poll_probes(&mut self) {
        // Read VAs
        for va_id in VAId::iter() {
            if va_id.enabled() {
                match self.read_va(va_id, va_id.v_probe(), va_id.a_probe()).await {
                    (ReadVAResult::Ok, res) => {
                        if let Some((voltage, amperage)) = res {
                            self.metrics
                                .update_va(va_id, ProbeStatus::Ok, voltage, amperage);
                        } else {
                            log::error!("Expected to have values for ReadVAResult::Ok");
                        }
                    }
                    (ReadVAResult::ReadError, _) => {
                        self.metrics
                            .update_va_status(va_id, ProbeStatus::Err(ProbeError::ReadError));
                    }
                    (ReadVAResult::ConversionError, _) => {
                        self.metrics
                            .update_va_status(va_id, ProbeStatus::Err(ProbeError::ConversionError));
                    }
                }

                self.tx_va(&va_id).await;
            }
        }

        self.tx_metrics().await;
    }

    async fn read_va<I>(
        &mut self,
        id: I,
        v_probeid: ProbeId,
        a_probeid: ProbeId,
    ) -> (ReadVAResult, Option<(f32, f32)>)
    where
        I: Debug,
    {
        match (
            self.probe.read(v_probeid).await,
            self.probe.read(a_probeid).await,
        ) {
            (Ok((_, Ok(voltage))), Ok((_, Ok(amperage)))) => {
                log::debug!(
                    "Read probes for {:?}: Voltage: {}, Amps: {}",
                    id,
                    voltage,
                    amperage
                );

                (ReadVAResult::Ok, Some((voltage.value(), amperage.value())))
            }
            (Ok(v_res), Ok(a_res)) => {
                if let (_, Err(e)) = v_res {
                    log::error!(
                        "Conversion error while reading voltage for probe '{:?}': {:?}",
                        id,
                        e
                    );
                }
                if let (_, Err(e)) = a_res {
                    log::error!(
                        "Conversion error while reading voltage for probe '{:?}': {:?}",
                        id,
                        e
                    );
                }

                (ReadVAResult::ConversionError, None)
            }
            (v_res, a_res) => {
                if let Err(e) = v_res {
                    log::error!("Failed to read voltage probe for '{:?}': {:?}", id, e);
                }
                if let Err(e) = a_res {
                    log::error!("Failed to read amperage probe for '{:?}': {:?}", id, e);
                }

                (ReadVAResult::ReadError, None)
            }
        }
    }

    async fn tx_metrics(&self) {
        if let Err(e) = self.m_tx.send_lossy(self.metrics.clone()).await {
            log::error!("Failed to send Metrics message: {:?}", e);
        }
    }

    async fn tx_va(&self, va_id: &VAId) {
        if let Some(va_val) = self.metrics.va_entries.get(va_id) {
            if let Err(e) = self.va_tx.send_lossy(va_val.clone()).await {
                log::error!("Failed to send VAMetricEntry message: {:?}", e);
            }
        }
    }
}

const MV_PER_V: f32 = 1_000.0;

pub fn format_voltage(mv: f32) -> String {
    let (value, unit) = if mv.abs() >= MV_PER_V {
        (mv / MV_PER_V, POWER_UNIT_VOLTS)
    } else {
        (mv, POWER_UNIT_MILLI_VOLTS)
    };

    format_power_unit(value, unit)
}

const MA_PER_A: f32 = 1_000.0;

pub fn format_amperage(ma: f32) -> String {
    let (value, unit) = if ma.abs() >= MV_PER_V {
        (ma / MA_PER_A, POWER_UNIT_AMPS)
    } else {
        (ma, POWER_UNIT_MILLI_AMPS)
    };

    format_power_unit(value, unit)
}

const MICROWATTS_TO_MILLIWATTS: f32 = 1_000.0;

pub fn calculate_milliwatts(mv: f32, ma: f32) -> f32 {
    (mv * ma) / MICROWATTS_TO_MILLIWATTS
}

const MW_PER_W: f32 = 1_000.0;

pub fn format_wattage(mw: f32) -> String {
    let (value, unit) = if mw.abs() >= MV_PER_V {
        (mw / MW_PER_W, POWER_UNIT_WATTS)
    } else {
        (mw, POWER_UNIT_MILLI_WATTS)
    };

    format_power_unit(value, unit)
}

pub fn format_power_unit(value: f32, unit: &str) -> String {
    let decimals = match value.abs() {
        x if x >= 100.0 => 0,
        x if x >= 10.0 => 1,
        x if x >= 1.0 => 2,
        _ => 3,
    };

    format!("{:.*} {}", decimals, value, unit)
}