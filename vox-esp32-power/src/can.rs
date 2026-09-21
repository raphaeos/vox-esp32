use alloc::borrow::ToOwned;
use anyhow::{anyhow, Result};
use async_channel::{bounded, Receiver, RecvError};
use vox_esp32_core::esp32_can::{
    CANManagerHandle, CANRxHandler, CANRxWorker, MessageId, Priority, Topic,
};
use vox_esp32_core::Controller;

use crate::metrics::Metrics;
use num_enum::TryFromPrimitive;
use vox_esp32_core::types::DeviceType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u32)]
pub enum MessageType {
    Metrics = 1,
}

impl MessageType {
    pub fn id(&self) -> u32 {
        *self as u32
    }
}

#[derive(Debug)]
pub enum Message {
    Metrics(Metrics),
}

pub fn register_handlers(
    controller: &mut Controller,
) -> Result<Receiver<(MessageId, MessageType, Message)>> {
    let (tx, rx) = bounded(10);
    //let tx2 = tx.clone();

    controller.add_can_handler(CANRxHandler::new(
        move |msg_id, payload| {
            let message_type: MessageType = msg_id.message_type.try_into()?;

            match msg_id.topic {
                Topic::Power => {
                    let mut message: Option<Message> = None;
                    match message_type {
                        MessageType::Metrics => {
                            let val: Metrics = postcard::from_bytes(payload)?;
                            message = Some(Message::Metrics(val));
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
async fn can_sender_task(metrics_rx: Receiver<Metrics>, mut can_handle: CANManagerHandle) {
    loop {
        if let Err(e) = can_sender_worker(&metrics_rx, &mut can_handle).await {
            log::error!("CAN Sender Task: Error encountered in worker: {}", e);
        }
    }
}

async fn can_sender_worker(
    metrics_rx: &Receiver<Metrics>,
    can_handle: &mut CANManagerHandle,
) -> Result<()> {
    let metrics = metrics_rx
        .recv()
        .await
        .map_err(|e| anyhow::anyhow!("CAN Sender Worker: Error receiving metrics: {}", e))?;

    let data = postcard::to_allocvec(&metrics)?;

    can_handle
        .tx(
            Priority::Default,
            Topic::Power,
            MessageType::Metrics.id(),
            data,
        )
        .await?;

    Ok(())
}

pub fn spawn_sender(
    controller: &mut Controller,
    metrics_rx: Receiver<Metrics>,
    can_handle: CANManagerHandle,
) -> Result<()> {
    controller.spawn(can_sender_task(metrics_rx, can_handle)?);

    Ok(())
}
