use crate::common::CoreError;
use crate::types::{Device, DeviceType};
use crate::Controller;
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use anyhow::{anyhow, Result};
use async_channel::{bounded, Receiver, RecvError, Sender};
use embassy_time::{Duration, Timer};
use esp_hal::twai::{BaudRate, EspTwaiFrame, ExtendedId, Id, TwaiMode, TwaiRx, TwaiTx};
use esp_hal::{twai, Async};
use futures::future::{self};
use futures::pin_mut;
use futures::select_biased;
use futures::FutureExt;
use num_enum::TryFromPrimitive;

const MAX_MESSAGE_SEQUENCE: u32 = 63;

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u32)]
pub enum Priority {
    Highest = 0,
    High = 1,
    Default = 2,
    Lowest = 3,
}

impl Priority {
    pub fn id(&self) -> u32 {
        *self as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u32)]
pub enum Topic {
    None = 1,
    Controller = 2,
    Power = 3,
    WaterHeater = 4,
}

impl Topic {
    pub fn id(&self) -> u32 {
        *self as u32
    }
}

#[derive(Debug, Clone)]
pub struct MessageId {
    pub priority: Priority,
    pub sequence_no: u32,
    pub topic: Topic,
    pub message_type: u32,
    pub sender: Device,
}

impl MessageId {
    pub fn new(
        priority: Priority,
        sequence_no: u32,
        topic: Topic,
        message_type: u32,
        sender: Device,
    ) -> Self {
        Self {
            priority,
            sequence_no,
            topic,
            message_type,
            sender,
        }
    }
}

impl TryFrom<ExtendedId> for MessageId {
    type Error = CoreError;

    fn try_from(value: ExtendedId) -> Result<Self, Self::Error> {
        let (priority, sequence_no, topic_type, message_type, device_type, sender_id) =
            unpack_msg_id(value);

        let priority = Priority::try_from(priority).map_err(|e| CoreError::Other(anyhow!(e)))?;

        let topic = Topic::try_from(topic_type).map_err(|e| CoreError::Other(anyhow!(e)))?;

        let device_type =
            DeviceType::try_from(device_type).map_err(|e| CoreError::Other(anyhow!(e)))?;

        Ok(Self {
            priority,
            sequence_no,
            topic,
            message_type,
            sender: Device::new(device_type, sender_id),
        })
    }
}

impl TryInto<ExtendedId> for MessageId {
    type Error = CoreError;

    fn try_into(self) -> Result<ExtendedId, Self::Error> {
        Ok(pack_msg_id(
            self.priority.id(),
            self.sequence_no,
            self.topic.id(),
            self.message_type,
            self.sender.device_type.id(),
            self.sender.device_id,
        )
        .map_err(|e| CoreError::Other(anyhow!(e)))?)
    }
}

#[derive(Clone)]
pub struct CANManagerHandle {
    tx: Sender<(Priority, Topic, u32, Vec<u8>)>,
    tx_request: Sender<(Priority, Topic, u32)>,
}

impl CANManagerHandle {
    pub async fn tx(
        &mut self,
        priority: Priority,
        topic: Topic,
        message_type_id: u32,
        payload: Vec<u8>,
    ) -> Result<()> {
        self.tx
            .send((priority, topic, message_type_id, payload))
            .await
            .map_err(|e| anyhow!("CAN tx channel closed: {}", e))
    }

    pub async fn tx_request(
        &mut self,
        priority: Priority,
        topic: Topic,
        message_type_id: u32,
    ) -> Result<()> {
        self.tx_request
            .send((priority, topic, message_type_id))
            .await
            .map_err(|e| anyhow!("CAN tx_request channel closed: {}", e))
    }
}

pub struct CANRxHandler {
    handle_cb: Box<dyn FnMut(&MessageId, &[u8]) -> Result<bool> + Send>,
    handle_request_cb: Box<dyn FnMut(&MessageId) -> Result<bool> + Send>,
}

impl CANRxHandler {
    pub fn new<F, R>(handle_cb: F, handle_request_cb: R) -> Self
    where
        F: FnMut(&MessageId, &[u8]) -> Result<bool> + Send + 'static,
        R: FnMut(&MessageId) -> Result<bool> + Send + 'static,
    {
        Self {
            handle_cb: Box::new(handle_cb),
            handle_request_cb: Box::new(handle_request_cb),
        }
    }

    pub async fn handle(&mut self, message_id: &MessageId, payload: &[u8]) -> Result<bool> {
        (self.handle_cb)(message_id, payload)
    }

    pub async fn handle_request(&mut self, message_id: &MessageId) -> Result<bool> {
        (self.handle_request_cb)(message_id)
    }
}

#[embassy_executor::task]
async fn can_rx_task(mut wkr: CANRxWorker) {
    wkr.run().await;
}

#[embassy_executor::task]
async fn can_tx_task(mut wkr: CANTxWorker) {
    wkr.run().await;
}

pub struct CANManager {
    handlers: Vec<CANRxHandler>,
}

impl CANManager {
    pub(crate) fn new() -> Self {
        Self { handlers: vec![] }
    }

    pub(crate) fn add_handler(&mut self, handler: CANRxHandler) -> &mut Self {
        self.handlers.push(handler);
        self
    }

    pub(crate) fn spawn(self, controller: &mut Controller) -> Result<CANManagerHandle> {
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
        let (tx_request, rx_request) = bounded(10);

        controller.spawn(can_rx_task(CANRxWorker::new(
            controller.device.clone(),
            can_rx,
            self.handlers,
        ))?);
        controller.spawn(can_tx_task(CANTxWorker::new(
            controller.device.clone(),
            can_tx,
            rx,
            rx_request,
        ))?);

        Ok(CANManagerHandle { tx, tx_request })
    }
}

pub struct CANRxWorker {
    device: Device,
    can_rx: TwaiRx<'static, Async>,
    handlers: Vec<CANRxHandler>,
}

impl CANRxWorker {
    fn new(device: Device, can_rx: TwaiRx<'static, Async>, handlers: Vec<CANRxHandler>) -> Self {
        Self {
            device,
            can_rx,
            handlers,
        }
    }

    async fn run(&mut self) {
        loop {
            match self.can_rx.receive_async().await {
                Ok(rx_frame) => {
                    log::trace!(
                        "Received Frame! ID: {:?}, Data: {:?}",
                        rx_frame.identifier(),
                        rx_frame.data()
                    );

                    if let Err(e) = self.handle_frame(rx_frame).await {
                        log::error!("Failed to handle CAN frame: {}", e);
                    }
                }
                Err(e) => {
                    log::error!("Error receiving CAN frame: {:?}", e);
                }
            }
        }
    }

    async fn handle_frame(&mut self, rx_frame: EspTwaiFrame) -> Result<()> {
        if let Id::Extended(id) = rx_frame.identifier() {
            let msg_id = MessageId::try_from(id)?;

            // Skip messages from ourself.
            if msg_id.sender.eq(&self.device) {
                return Ok(());
            }

            for handler in self.handlers.iter_mut() {
                if rx_frame.is_remote_request() {
                    match handler.handle_request(&msg_id).await {
                        Ok(true) => continue,
                        Ok(false) => return Ok(()), // Abort chain.
                        Err(e) => {
                            log::error!("Failed to call handle_request to handle CAN frame: {}", e);
                        }
                    }
                } else {
                    match handler.handle(&msg_id, rx_frame.data()).await {
                        Ok(true) => continue,
                        Ok(false) => return Ok(()), // Abort chain.
                        Err(e) => {
                            log::error!("Failed to call handler to handle CAN frame: {}", e);
                        }
                    }
                }
            }
        } else {
            log::warn!(
                "Received unexpected standard length CAN frame Id: {:?}",
                rx_frame.identifier()
            );
        }

        Ok(())
    }
}

pub struct CANTxWorker {
    device: Device,
    can_tx: TwaiTx<'static, Async>,
    rx: Receiver<(Priority, Topic, u32, Vec<u8>)>,
    rx_request: Receiver<(Priority, Topic, u32)>,
    seq: u32,
}

impl CANTxWorker {
    fn new(
        device: Device,
        can_tx: TwaiTx<'static, Async>,
        rx: Receiver<(Priority, Topic, u32, Vec<u8>)>,
        rx_request: Receiver<(Priority, Topic, u32)>,
    ) -> Self {
        Self {
            device,
            can_tx,
            rx,
            rx_request,
            seq: 0,
        }
    }

    async fn run(&mut self) {
        loop {
            let rx_fut = self.rx.recv().fuse();
            let rx_request_fut = self.rx_request.recv().fuse();

            pin_mut!(rx_fut, rx_request_fut);

            select_biased! {
                msg_res = rx_fut => {
                    match msg_res {
                        Ok((priority, topic, message_type, payload)) => {
                            if let Err(e) = self.handle_send(priority, topic, message_type, payload).await {
                                log::error!("Failed to handle CANTxWorker send (rx): {}", e);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to receive from CANTxWorker channel (rx): {}", e);
                        }
                    }
                }
                msg_res = rx_request_fut => {
                    match msg_res {
                        Ok((priority, topic, message_type)) => {
                            if let Err(e) = self.handle_send_request(priority, topic, message_type).await {
                                log::error!("Failed to handle CANTxWorker send request (rx_request): {}", e);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to receive from CANTxWorker channel (rx_request): {}", e);
                        }
                    }
                }
            }
        }
    }

    async fn handle_send(
        &mut self,
        priority: Priority,
        topic: Topic,
        message_type: u32,
        payload: Vec<u8>,
    ) -> Result<()> {
        let frame_id = self.make_frame_id(priority, topic, message_type, true)?;

        let tx_frame =
            EspTwaiFrame::new(frame_id, &payload).ok_or(anyhow!("Failed to create frame"))?;

        self.send_can_frame(&tx_frame).await
    }

    async fn handle_send_request(
        &mut self,
        priority: Priority,
        topic: Topic,
        message_type: u32,
    ) -> Result<()> {
        let frame_id = self.make_frame_id(priority, topic, message_type, false)?;

        let tx_frame =
            EspTwaiFrame::new_remote(frame_id, 0).ok_or(anyhow!("Failed to create frame"))?;

        self.send_can_frame(&tx_frame).await
    }

    async fn send_can_frame(&mut self, frame: &EspTwaiFrame) -> Result<()> {
        self.can_tx
            .transmit_async(frame)
            .await
            .map_err(|e| anyhow!("Failed to transmit CAN frame: {:?}", e))?;

        Ok(())
    }

    fn make_frame_id(
        &mut self,
        priority: Priority,
        topic: Topic,
        message_type: u32,
        use_seq: bool,
    ) -> Result<Id> {
        let seq = if use_seq {
            if self.seq >= MAX_MESSAGE_SEQUENCE {
                self.seq = 1;
            } else {
                self.seq += 1;
            }

            self.seq
        } else {
            0
        };

        let msg_id = MessageId::new(priority, seq, topic, message_type, self.device.clone());
        let extended_id: ExtendedId = msg_id.try_into()?;

        Ok(Id::Extended(extended_id))
    }
}

// Utils

fn pack_msg_id(
    priority: u32,     // 2 bits (0-3)
    sequence_no: u32,  // 6 bits (0-63)
    topic_type: u32,   // 5 bits (0-31)
    message_type: u32, // 6 bits (0-63)
    device_type: u32,  // 4 bits (0-15)
    sender_id: u32,    // 6 bits (0-63)
) -> Result<ExtendedId> {
    let raw_id = (priority << 27)
        | (sequence_no << 21)
        | (topic_type << 16)
        | (message_type << 10)
        | (device_type << 6)
        | sender_id;

    ExtendedId::new(raw_id).ok_or(anyhow!(CoreError::CANMsgIdExceeded))
}

fn unpack_msg_id(id: ExtendedId) -> (u32, u32, u32, u32, u32, u32) {
    let raw_id = id.as_raw();

    let priority = (raw_id >> 27) & 0x3; // 2 bits
    let sequence_no = (raw_id >> 21) & 0x3F; // 6 bits
    let topic_type = (raw_id >> 16) & 0x1F; // 5 bits
    let message_type = (raw_id >> 10) & 0x3F; // 6 bits
    let device_type = (raw_id >> 6) & 0xF; // 4 bits
    let sender_id = raw_id & 0x3F; // 6 bits

    (
        priority,
        sequence_no,
        topic_type,
        message_type,
        device_type,
        sender_id,
    )
}
