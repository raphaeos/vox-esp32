use crate::common::CoreError;
use crate::types::DeviceType;
use crate::Controller;
use anyhow::{anyhow, Result};
use async_channel::{bounded, Receiver, Sender};
use embassy_time::{Duration, Timer};
use embedded_can::ExtendedId;
use esp_hal::twai::{BaudRate, TwaiMode, TwaiRx, TwaiTx};
use esp_hal::{twai, Async};

#[derive(Debug)]
pub struct MessageId {
    device_type: DeviceType,
    device_id: u32,
    message_type: u32,
    sequence_no: u32,
}

impl MessageId {
    pub fn new(
        device_type: DeviceType,
        device_id: u32,
        message_type: u32,
        sequence_no: u32,
    ) -> Self {
        Self {
            device_type,
            device_id,
            message_type,
            sequence_no,
        }
    }
}

impl TryFrom<ExtendedId> for MessageId {
    type Error = CoreError;

    fn try_from(value: ExtendedId) -> Result<Self, Self::Error> {
        let (device_type_n, device_id, message_type, sequence_no) = unpack_msg_id(value);

        let device_type =
            DeviceType::try_from(device_type_n).map_err(|e| CoreError::Other(anyhow!(e)))?;

        Ok(Self {
            device_type,
            device_id,
            message_type,
            sequence_no,
        })
    }
}

impl TryInto<ExtendedId> for MessageId {
    type Error = CoreError;

    fn try_into(self) -> Result<ExtendedId, Self::Error> {
        Ok(pack_msg_id(
            self.device_type.id(),
            self.device_id,
            self.message_type,
            self.sequence_no,
        )
        .map_err(|e| CoreError::Other(anyhow!(e)))?)
    }
}

#[derive(Clone)]
pub struct CANManagerHandle {
    tx: Sender<()>,
}

impl CANManagerHandle {}

#[embassy_executor::task]
async fn can_rx_task(mut wkr: CANRxWorker) {
    wkr.run().await;
}

#[embassy_executor::task]
async fn can_tx_task(mut wkr: CANTxWorker) {
    wkr.run().await;
}

pub struct CANManager {}

impl CANManager {
    pub(crate) fn spawn(controller: &mut Controller) -> Result<CANManagerHandle> {
        log::info!("Vox ESP32 Core: CAN Manager started");

        const TWAI_BAUDRATE: BaudRate = BaudRate::B1000K;

        let can = twai::TwaiConfiguration::new(
            controller
                .peripherals
                .TWAI0
                .take()
                .ok_or(CoreError::PeripheralTaken("TWAI0"))?,
            controller
                .peripherals
                .GPIO3
                .take()
                .ok_or(CoreError::PeripheralTaken("GPIO3"))?,
            controller
                .peripherals
                .GPIO8
                .take()
                .ok_or(CoreError::PeripheralTaken("GPIO8"))?,
            TWAI_BAUDRATE,
            TwaiMode::Normal,
        )
        .into_async()
        .start();

        let (can_rx, can_tx) = can.split();

        let (tx, rx) = bounded(10);

        controller.spawn(can_rx_task(CANRxWorker { can_rx })?);
        controller.spawn(can_tx_task(CANTxWorker { can_tx, rx })?);

        Ok(CANManagerHandle { tx })
    }
}

pub struct CANRxWorker {
    can_rx: TwaiRx<'static, Async>,
}

impl CANRxWorker {
    async fn run(&mut self) {
        loop {
            match self.can_rx.receive_async().await {
                Ok(rx_frame) => {
                    log::debug!(
                        "Received Frame! ID: {:?}, Data: {:?}",
                        rx_frame.identifier(),
                        rx_frame.data()
                    );
                }
                Err(e) => {
                    log::debug!("Error receiving CAN frame: {:?}", e);
                }
            }
        }
    }
}

pub struct CANTxWorker {
    can_tx: TwaiTx<'static, Async>,
    rx: Receiver<()>,
}

impl CANTxWorker {
    async fn run(&mut self) {
        loop {
            Timer::after(Duration::from_millis(1000)).await;
        }
    }
}

// Utils

/*
    let device_type: u32 = 0x15;    // Max 0x1F  (32 values)
    let device_id: u32 = 250;       // Max 0xFF  (256 values)
    let message_type: u32 = 0x7E;   // Max 0x7F  (128 values)
    let sequence_no: u32 = 2000;    // Max 0x7FF (2048 values)
*/

fn pack_msg_id(
    device_type: u32,
    device_id: u32,
    message_type: u32,
    sequence_no: u32,
) -> Result<ExtendedId> {
    // Shift fields into position
    let raw_id = (device_type << 24) | (device_id << 16) | (message_type << 11) | sequence_no;

    ExtendedId::new(raw_id).ok_or(anyhow!(CoreError::CANMsgIdExceeded))
}

fn unpack_msg_id(id: ExtendedId) -> (u32, u32, u32, u32) {
    let raw_id = id.as_raw();

    // Mask out each field and shift them back down to zero
    let device_type = (raw_id >> 24) & 0x1F; // 5 bits
    let device_id = (raw_id >> 16) & 0xFF; // 8 bits
    let message_type = (raw_id >> 11) & 0x7F; // 7 bits
    let sequence_no = raw_id & 0x7FF; // 11 bits

    (device_type, device_id, message_type, sequence_no)
}
