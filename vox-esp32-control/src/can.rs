use anyhow::Result;
use async_channel::{bounded, Receiver};
use vox_esp32_core::{CANManagerHandle, Controller, MessageId};
use vox_esp32_power::can::{Message, MessageType};
use vox_esp32_power::metrics::Metrics;

pub(crate) async fn start(controller: &mut Controller) -> Result<()> {
    CANManager::spawn(controller).await
}

struct CANManager {
    can_handle: CANManagerHandle,
    power_can_rx: Receiver<(MessageId, MessageType, Message)>,
    power_metrics: Metrics,
}

impl CANManager {
    fn new(
        can_handle: CANManagerHandle,
        power_can_rx: Receiver<(MessageId, MessageType, Message)>,
    ) -> Self {
        Self {
            can_handle,
            power_can_rx,
            power_metrics: Metrics::new(),
        }
    }

    async fn spawn(controller: &mut Controller) -> Result<()> {
        let power_can_rx = vox_esp32_power::can::register_handlers(controller)?;

        let can_handle = controller.start_can()?;

        let mgr = CANManager::new(can_handle, power_can_rx);

        Ok(())
    }
}
