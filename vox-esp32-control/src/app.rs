use alloc::vec::Vec;
use anyhow::Result;
use crate::display::{DisplayLCDPeripherals, DisplayTouchPeripherals};
use crate::ui::AppWindow;
use crate::{display, ui};
use async_channel::Receiver;
use core::ptr::addr_of_mut;
use embassy_executor::Spawner;
use esp_rtos::embassy::Executor;
use static_cell::StaticCell;
use vox_esp32_core::async_channel::{lossy_bounded, LossyChannel};
use vox_esp32_core::common::CoreError;
use vox_esp32_core::Controller;
use vox_esp32_power::metrics::{Metrics, VAId};
use vox_esp32_power::powmr_mppt::MPPTSummary;
use strum::IntoEnumIterator;

static mut APP_CORE_STACK: esp_hal::system::Stack<65536> = esp_hal::system::Stack::new();

static APP_CORE_EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[embassy_executor::task]
async fn app_task(
    spawner: Spawner,
    app_update_rx: Receiver<AppSyncState>,
    lcd_peripherals: DisplayLCDPeripherals,
    touch_peripherals: DisplayTouchPeripherals,
) {
    log::info!("Starting app on core 1");

    let (window, ui) = ui::init().expect("Failed to init UI");

    display::spawn(&spawner, window, lcd_peripherals, touch_peripherals)
        .await
        .expect("Failed to spawn app task");

    let sync_mgr = AppSyncManager::new(ui, app_update_rx);

    spawner.spawn(app_sync_task(sync_mgr).expect("Failed to spawn app sync task"));
}

pub(crate) async fn start(
    controller: &mut Controller,
) -> anyhow::Result<LossyChannel<AppSyncState>> {
    let lcd_peripherals = DisplayLCDPeripherals::new(controller)?;
    let touch_peripherals = DisplayTouchPeripherals::new(controller)?;

    let (app_update_tx, app_update_rx) = lossy_bounded(2);

    let cpu_ctrl = controller
        .peripherals
        .CPU_CTRL
        .take()
        .ok_or(CoreError::PeripheralTaken("CPU_CTRL"))?;

    let from_cpu_intr1 = controller
        .peripherals
        .FROM_CPU_INTR1
        .take()
        .ok_or(CoreError::PeripheralTaken("FROM_CPU_INTR1"))?;

    let app_core_stack = unsafe { addr_of_mut!(APP_CORE_STACK).as_mut().unwrap() };

    esp_rtos::start_second_core(cpu_ctrl, from_cpu_intr1, app_core_stack, move || {
        let executor = APP_CORE_EXECUTOR.init(Executor::new());

        executor.run(|spawner| {
            spawner.spawn(
                app_task(spawner, app_update_rx, lcd_peripherals, touch_peripherals).unwrap(),
            );
        })
    });

    Ok(app_update_tx)
}

#[embassy_executor::task]
async fn app_sync_task(mut mgr: AppSyncManager) {
    mgr.run().await
}

pub(crate) struct AppSyncState {
    power_metrics: Metrics,
    power_mppt: Option<MPPTSummary>,
}

impl AppSyncState {
    pub fn new(power_metrics: Metrics, power_mppt: Option<MPPTSummary>) -> Self {
        Self {
            power_metrics,
            power_mppt,
        }
    }
}

pub(crate) struct AppSyncManager {
    ui: AppWindow,
    app_update_rx: Receiver<AppSyncState>,
}

impl AppSyncManager {
    fn new(ui: AppWindow, app_update_rx: Receiver<AppSyncState>) -> Self {
        Self { ui, app_update_rx }
    }

    async fn run(&mut self) {
        loop {
            match self.app_update_rx.recv().await {
                Ok(update) => {
                    if let Err(e) = self.sync_app_state(update).await {
                        log::error!("Failed to sync app state: {:?}", e);
                    }
                }
                Err(e) => {
                    log::error!("Failed to receive app update from tx: {:?}", e);
                }
            }
        }
    }

    async fn sync_app_state(&mut self, update: AppSyncState) -> Result<()> {

        // Iterate in order so we create the data in order
        let mut va_entries: Vec<ui::PowerVAEntry> = Vec::new();
        for va_id in VAId::iter() {
            if let Some(entry) = update.power_metrics.va_entries.get(&va_id) {
                let ui_entry: ui::PowerVAEntry = entry.into();
                va_entries.push(ui_entry);
            }
        }

        let power_state = ui::PowerState {
            va_entries: slint::ModelRc::from(&va_entries[..]),
        };

        Ok(())
    }
}
