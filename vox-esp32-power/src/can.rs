use anyhow::{anyhow, Result};
use async_channel::{bounded, Receiver};
use futures::pin_mut;
use futures::select_biased;
use futures::FutureExt;
use vox_esp32_core::esp32_can::{CANManagerHandle, CANRxHandler, MessageId, Priority, Topic};
use vox_esp32_core::Controller;

use crate::metrics::{VAId, VAMetricEntry};
use crate::powmr_mppt::{MPPTResult, MPPTState, MPPTSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    MPPTSummary,
    VAMetricEntry(VAId),
}

impl MessageType {
    pub fn id(&self) -> u32 {
        match self {
            MessageType::MPPTSummary => 2,
            MessageType::VAMetricEntry(va_id) => 10 + (*va_id as u32),
        }
    }
}

impl TryFrom<u32> for MessageType {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            2 => Ok(MessageType::MPPTSummary),
            v if v >= 10 && v < 35 => {
                let va_id: u8 = (value - 10) as u8;
                let va_id: VAId = va_id.try_into()?;

                Ok(MessageType::VAMetricEntry(va_id))
            }
            _ => Err(anyhow!("Invalid id {} for (Power) MessageType", value)),
        }
    }
}

#[derive(Debug)]
pub enum Message {
    Metrics(VAMetricEntry),
    MPPTSummary(MPPTSummary),
}

pub fn register_handlers(
    controller: &mut Controller,
) -> Result<Receiver<(MessageId, MessageType, Message)>> {
    let (tx, rx) = bounded(10);
    //let tx2 = tx.clone();

    controller.add_can_handler(CANRxHandler::new(
        move |msg_id, payload| {
            match msg_id.topic {
                Topic::Power => {
                    let message_type: MessageType = msg_id.message_type.try_into()?;
                    let message: Option<Message>;
                    match message_type {
                        MessageType::MPPTSummary => {
                            message = Some(Message::MPPTSummary(MPPTSummary::from_bytes(payload)?));
                        }
                        MessageType::VAMetricEntry(va_id) => {
                            message =
                                Some(Message::Metrics(VAMetricEntry::from_bytes(va_id, payload)?));
                        }
                    }

                    if let Some(message) = message {
                        tx.try_send((msg_id.clone(), message_type, message))
                            .map_err(|e| anyhow!("CAN handler channel full: {:?}", e))?;
                    }
                }
                _ => {}
            }

            Ok(true)
        },
        move |msg_id| {
            /*
            let message_type: MessageType = msg_id.message_type.try_into()?;

            match msg_id.topic {
                Topic::Power => {
                    tx2.try_send((msg_id.clone(), message_type, None))
                        .map_err(|e| anyhow!("CAN handler channel full: {:?}", e))?;
                }
                _ => {}
            }
            */

            Ok(true)
        },
    ))?;

    Ok(rx)
}

#[embassy_executor::task]
async fn can_sender_task(
    va_metric_rx: Receiver<VAMetricEntry>,
    mppt_rx: Receiver<MPPTResult<MPPTState>>,
    mut can_handle: CANManagerHandle,
) {
    loop {
        if let Err(e) = can_sender_worker(&va_metric_rx, &mppt_rx, &mut can_handle).await {
            log::error!("CAN Sender Task: Error encountered in worker: {}", e);
        }
    }
}

async fn can_sender_worker(
    va_metric_rx: &Receiver<VAMetricEntry>,
    mppt_rx: &Receiver<MPPTResult<MPPTState>>,
    can_handle: &mut CANManagerHandle,
) -> Result<()> {
    let va_metric_rx_fut = va_metric_rx.recv().fuse();
    let mppt_rx_fut = mppt_rx.recv().fuse();

    pin_mut!(va_metric_rx_fut, mppt_rx_fut);

    select_biased! {
        msg_res = mppt_rx_fut => {
            match msg_res {
                Ok(mppt_res) => can_sender_worker_va_mppt_result(can_handle, mppt_res).await?,
                Err(e) => return Err(anyhow::anyhow!("CAN Sender Worker: Error receiving MPPT result: {}", e))
            }
        },
        msg_res = va_metric_rx_fut => {
            match msg_res {
                Ok(va_entry) => can_sender_worker_va_metrics(can_handle, va_entry).await?,
                Err(e) => return Err(anyhow::anyhow!("CAN Sender Worker: Error receiving Metrics: {}", e))
            }
        },
    }

    Ok(())
}

async fn can_sender_worker_va_mppt_result(
    can_handle: &mut CANManagerHandle,
    mppt_res: MPPTResult<MPPTState>,
) -> Result<()> {
    let mppt_summary: MPPTSummary = mppt_res.into();

    can_handle
        .tx(
            Priority::Default,
            Topic::Power,
            MessageType::MPPTSummary.id(),
            mppt_summary.to_bytes()?,
        )
        .await
}

async fn can_sender_worker_va_metrics(
    can_handle: &mut CANManagerHandle,
    va_entry: VAMetricEntry,
) -> Result<()> {
    can_handle
        .tx(
            Priority::Default,
            Topic::Power,
            MessageType::VAMetricEntry(va_entry.id).id(),
            va_entry.to_bytes()?,
        )
        .await
}

pub fn spawn_sender(
    controller: &mut Controller,
    va_metric_rx: Receiver<VAMetricEntry>,
    mppt_rx: Receiver<MPPTResult<MPPTState>>,
    can_handle: CANManagerHandle,
) -> Result<()> {
    controller.spawn(can_sender_task(va_metric_rx, mppt_rx, can_handle)?);

    Ok(())
}
