use crate::powmr_mppt::{MPPTManager, MPPTResult, MPPTState};
use alloc::collections::BTreeMap;
use anyhow::Result;
use async_channel::{bounded, Receiver, Sender};
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
    ADSMultiProbe, Address, ProbeType, ProbeValue, ACS758LCB_050B, V5_1,
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
}

impl ProbeId {
    fn pv_id(&self) -> PvId {
        match self {
            ProbeId::PvV1 => PvId::Pv1,
            ProbeId::PvA1 => PvId::Pv1,
            ProbeId::PvV2 => PvId::Pv2,
            ProbeId::PvA2 => PvId::Pv2,
            ProbeId::PvV3 => PvId::Pv3,
            ProbeId::PvA3 => PvId::Pv3,
        }
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
        };

        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Serialize, Deserialize, EnumIter)]
pub enum PvId {
    Pv1,
    Pv2,
    Pv3,
}

impl PvId {
    fn enabled(&self) -> bool {
        match self {
            PvId::Pv1 => true,
            PvId::Pv2 => true,
            PvId::Pv3 => true,
        }
    }

    fn v_probe(&self) -> ProbeId {
        match self {
            PvId::Pv1 => ProbeId::PvV1,
            PvId::Pv2 => ProbeId::PvV2,
            PvId::Pv3 => ProbeId::PvV3,
        }
    }

    fn a_probe(&self) -> ProbeId {
        match self {
            PvId::Pv1 => ProbeId::PvA1,
            PvId::Pv2 => ProbeId::PvA2,
            PvId::Pv3 => ProbeId::PvA3,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Metrics {
    pv_entries: BTreeMap<PvId, PVMetricEntry>,
}

impl Metrics {
    fn new() -> Self {
        Self {
            pv_entries: BTreeMap::new(),
        }
    }

    fn update_pv(&mut self, id: PvId, status: PvStatus, voltage: f32, amperage: f32) {
        self.pv_entries
            .entry(id)
            .and_modify(|entry| {
                entry.status = status;
                entry.voltage = voltage;
                entry.amperage = amperage;
            })
            .or_insert_with(|| PVMetricEntry::new(id, status, voltage, amperage));
    }

    fn update_pv_status(&mut self, id: PvId, status: PvStatus) {
        self.pv_entries
            .entry(id)
            .and_modify(|entry| {
                entry.status = status;
            })
            .or_insert_with(|| PVMetricEntry::new(id, status, 0.0, 0.0));
    }
}

#[derive(Error, Copy, Clone, Debug, Serialize, Deserialize)]
pub enum PvError {
    #[error("Read error")]
    ReadError,
    #[error("Conversion error")]
    ConversionError,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum PvStatus {
    Ok,
    Err(PvError),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PVMetricEntry {
    id: PvId,
    status: PvStatus,
    voltage: f32,
    amperage: f32,
}

impl PVMetricEntry {
    fn new(id: PvId, status: PvStatus, voltage: f32, amperage: f32) -> Self {
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
    tx: Sender<MPPTResult<MPPTState>>,
    mppt_rx: Receiver<MPPTResult<MPPTState>>,
    probe: ADSMultiProbe<ProbeId>,
    led: LEDManagerHandle,
    metrics: Metrics,
}

impl MetricsManager {
    pub(crate) fn spawn(controller: &mut Controller) -> Result<Receiver<MPPTResult<MPPTState>>> {
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
            if probe_id.pv_id().enabled() {
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

    async fn process_mppt_msg(&mut self, msg: MPPTResult<MPPTState>) {}

    async fn poll_probes(&mut self) {
        for pvid in PvId::iter() {
            match (
                self.probe.read(pvid.v_probe()).await,
                self.probe.read(pvid.a_probe()).await,
            ) {
                (Ok((_, Ok(voltage))), Ok((_, Ok(amperage)))) => {
                    log::debug!(
                        "Read probes for {:?}: Voltage: {}, Amps: {}",
                        pvid,
                        voltage,
                        amperage
                    );

                    self.metrics
                        .update_pv(pvid, PvStatus::Ok, voltage.value(), amperage.value());
                }
                (Ok(v_res), Ok(a_res)) => {
                    if let (_, Err(e)) = v_res {
                        log::error!(
                            "Conversion error while reading voltage for probe '{:?}': {:?}",
                            pvid,
                            e
                        );
                    }
                    if let (_, Err(e)) = a_res {
                        log::error!(
                            "Conversion error while reading voltage for probe '{:?}': {:?}",
                            pvid,
                            e
                        );
                    }

                    self.metrics
                        .update_pv_status(pvid, PvStatus::Err(PvError::ConversionError));
                }
                (v_res, a_res) => {
                    if let Err(e) = v_res {
                        log::error!("Failed to read voltage probe for '{:?}': {:?}", pvid, e);
                    }
                    if let Err(e) = a_res {
                        log::error!("Failed to read amperage probe for '{:?}': {:?}", pvid, e);
                    }

                    self.metrics
                        .update_pv_status(pvid, PvStatus::Err(PvError::ReadError));
                }
            }
        }
    }
}
