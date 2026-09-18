use crate::powmr_mppt::{MPPTError, MPPTErrorExt, MPPTManager, MPPTResult, MPPTState};
use alloc::collections::BTreeMap;
use anyhow::Result;
use async_channel::{bounded, Receiver, Sender};
use core::fmt::{Debug, Display};
use embassy_time::{Duration, Timer};
use embedded_ads111x::InputMultiplexer;
use futures::pin_mut;
use futures::select_biased;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use thiserror::Error;
use vox_esp32_core::ads111x::{
    ADSMultiProbe, Address, ProbeType, ProbeValue, ACS758LCB_050B, QNHCK1_21_300_AMPS, V5_1,
};
use vox_esp32_core::common::CoreError;
use vox_esp32_core::esp32_led::LEDManagerHandle;
use vox_esp32_core::Controller;

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum VAType {
    Pv,
    Batt,
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Serialize, Deserialize, EnumIter)]
pub enum VAId {
    Pv1,
    Pv2,
    Pv3,
    Batt,
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
    va_entries: BTreeMap<VAId, VAMetricEntry>,
    mppt_state: Option<MPPTState>,
    mppt_error: Option<MPPTErrorExt>,
}

impl Metrics {
    fn new() -> Self {
        Self {
            va_entries: BTreeMap::new(),
            mppt_state: None,
            mppt_error: None,
        }
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

    fn update_mppt(&mut self, res: MPPTResult<MPPTState>) {
        match res {
            Ok(state) => {
                self.mppt_state = Some(state);
                self.mppt_error = None;
            }
            Err(e) => {
                self.mppt_error = Some(e.into());
                self.mppt_state = None
            }
        }
    }
}

#[derive(Error, Copy, Clone, Debug, Serialize, Deserialize)]
pub enum ProbeError {
    #[error("Read error")]
    ReadError,
    #[error("Conversion error")]
    ConversionError,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum ProbeStatus {
    Init,
    Ok,
    Err(ProbeError),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BattMetric {
    status: ProbeStatus,
    voltage: f32,
    amperage: f32,
}

impl Default for BattMetric {
    fn default() -> Self {
        Self {
            status: ProbeStatus::Init,
            voltage: 0.0,
            amperage: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VAMetricEntry {
    id: VAId,
    status: ProbeStatus,
    voltage: f32,
    amperage: f32,
}

impl VAMetricEntry {
    fn new(id: VAId, status: ProbeStatus, voltage: f32, amperage: f32) -> Self {
        Self {
            id,
            status,
            voltage,
            amperage,
        }
    }
}

#[embassy_executor::task]
async fn metrics_task(mut mgr: MetricsManager) {
    mgr.run().await;
}

pub struct MetricsManager {
    tx: Sender<Metrics>,
    mppt_rx: Receiver<MPPTResult<MPPTState>>,
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
    pub(crate) fn spawn(controller: &mut Controller) -> Result<Receiver<Metrics>> {
        let mppt_rx = MPPTManager::spawn(controller)?;

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

        let (tx, rx) = bounded(10);

        let led = controller.led.clone();

        let mgr = MetricsManager {
            tx,
            mppt_rx,
            probe,
            led,
            metrics: Metrics::new(),
        };

        controller.spawn(metrics_task(mgr)?);

        Ok(rx)
    }

    async fn run(&mut self) {
        log::info!("Vox ESP32 Solar: Metrics Manager started");

        loop {
            let mppt_rx_fut = self.mppt_rx.recv().fuse();

            let timeout_fut = Timer::after(Duration::from_millis(1000)).fuse();

            pin_mut!(mppt_rx_fut, timeout_fut);

            select_biased! {
                msg_res = mppt_rx_fut => {
                    match msg_res {
                        Ok(msg) => {
                            self.process_mppt_msg(msg).await;
                        }
                        Err(_) => {
                            // Channel was closed, exit the loop safely
                            break;
                        }
                    }
                },
                _ = timeout_fut => {
                    self.poll_probes().await;
                }
            }
        }
    }

    async fn process_mppt_msg(&mut self, msg: MPPTResult<MPPTState>) {
        self.metrics.update_mppt(msg);

        self.tx_metrics().await;
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
        if let Err(e) = self.tx.send(self.metrics.clone()).await {
            log::error!("Failed to send metrics message: {:?}", e);
        }
    }
}
