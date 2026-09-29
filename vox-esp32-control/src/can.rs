use crate::app::AppSyncState;
use anyhow::Result;
use async_channel::Receiver;
use embassy_time::{Duration, Timer};
use esp_hal::time::Instant;
use futures::future::{self};
use futures::pin_mut;
use futures::select_biased;
use futures::FutureExt;
use vox_esp32_core::async_channel::LossyChannel;
use vox_esp32_core::{CANManagerHandle, Controller, MessageId};
use vox_esp32_power::can::{Message, MessageType};
use vox_esp32_power::metrics::Metrics;
use vox_esp32_power::powmr_mppt::MPPTSummary;

const UI_UPDATE_DELAY_MS: u64 = 100; // Ms to wait before sending an update
const UI_UPDATE_DELAY_MIN_MS: u64 = 10; // Ms to wait before sending an update (when already exceeded waiting delay + period)
const UI_UPDATE_DELAY_MAX_MS: u64 = 1000; // Max ms to wait before just sending update (if we are getting constant messages for example)
const UI_UPDATE_PERIOD_MS: u64 = 500; // Min ms to pass before a new update

pub(crate) async fn start(
    controller: &mut Controller,
    app_update_tx: LossyChannel<AppSyncState>,
) -> Result<()> {
    CANManager::spawn(controller, app_update_tx).await
}

#[embassy_executor::task]
async fn can_task(mut mgr: CANManager) {
    mgr.run().await;
}

struct CANManager {
    can_handle: CANManagerHandle,
    app_update_tx: LossyChannel<AppSyncState>,
    power_can_rx: Receiver<(MessageId, MessageType, Message)>,
    power_metrics: Metrics,
    power_mppt: Option<MPPTSummary>,
    have_app_update: bool,
    last_app_update: Instant,
}

impl CANManager {
    fn new(
        can_handle: CANManagerHandle,
        app_update_tx: LossyChannel<AppSyncState>,
        power_can_rx: Receiver<(MessageId, MessageType, Message)>,
    ) -> Self {
        Self {
            can_handle,
            app_update_tx,
            power_can_rx,
            power_metrics: Metrics::new(),
            power_mppt: None,
            have_app_update: false,
            last_app_update: Instant::now(),
        }
    }

    async fn spawn(
        controller: &mut Controller,
        app_update_tx: LossyChannel<AppSyncState>,
    ) -> Result<()> {
        let power_can_rx = vox_esp32_power::can::register_handlers(controller)?;

        let can_handle = controller.start_can()?;

        let mgr = CANManager::new(can_handle, app_update_tx, power_can_rx);

        controller.spawn(can_task(mgr)?);

        Ok(())
    }

    async fn run(&mut self) {
        log::info!("Vox ESP32 Control: CAN Manager started");

        loop {
            let mut app_update_delay: Option<Duration> = None;
            if self.have_app_update {
                let elapsed = self.last_app_update.elapsed().as_millis();
                if elapsed >= UI_UPDATE_PERIOD_MS {
                    if elapsed > UI_UPDATE_DELAY_MAX_MS {
                        self.dispatch_app_update().await;
                    } else if elapsed >= UI_UPDATE_DELAY_MS + UI_UPDATE_PERIOD_MS {
                        app_update_delay = Some(Duration::from_millis(UI_UPDATE_DELAY_MIN_MS));
                    } else {
                        app_update_delay = Some(Duration::from_millis(UI_UPDATE_DELAY_MS));
                    }
                }
            }

            let power_can_rx_fut = self.power_can_rx.recv().fuse();

            let ui_update_timeout_fut = if let Some(delay) = app_update_delay {
                Timer::after(delay).left_future()
            } else {
                future::pending::<()>().right_future()
            }
            .fuse();

            pin_mut!(power_can_rx_fut, ui_update_timeout_fut);

            select_biased! {
                msg_res = power_can_rx_fut => {
                    match msg_res {
                        Ok((msg_id, msg_type, msg)) => {
                            self.process_power_msg(msg_id, msg_type, msg).await;
                        }
                        Err(e) => {
                            log::warn!("Receive error while trying to get a CAN power msg: {:?}", e);
                        }
                    }
                }
                _ = ui_update_timeout_fut => {
                    self.dispatch_app_update().await;
                }
            }
        }
    }

    async fn process_power_msg(
        &mut self,
        _msg_id: MessageId,
        _msg_type: MessageType,
        msg: Message,
    ) {
        match msg {
            Message::Metrics(m) => {
                self.power_metrics.apply_va(m.id, m);
                self.have_app_update = true;
            }
            Message::MPPTSummary(m) => {
                self.power_mppt = Some(m);
                self.have_app_update = true;
            }
        }
    }

    async fn dispatch_app_update(&mut self) {
        if let Err(e) = self
            .app_update_tx
            .send_lossy(AppSyncState::new(
                self.power_metrics.clone(),
                self.power_mppt.clone(),
            ))
            .await
        {
            log::warn!("Failed to send app update tx: {:?}", e)
        }

        self.last_app_update = Instant::now();
        self.have_app_update = false;
    }
}
