use crate::display::{DisplayLCDPeripherals, DisplayTouchPeripherals};
use crate::ui::{extract_power_va_entries, AppWindow};
use crate::{display, ui};
use alloc::vec::Vec;
use anyhow::{anyhow, Result};
use async_channel::Receiver;
use core::ops::Sub;
use core::ptr::addr_of_mut;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::time::Instant;
use esp_rtos::embassy::Executor;
use ft6336u_dd::TouchData;
use futures::future::{self};
use futures::pin_mut;
use futures::select_biased;
use futures::FutureExt;
use slint::ComponentHandle;
use slint::PlatformError::SetPlatformError;
use static_cell::StaticCell;
use vox_esp32_core::async_channel::{lossy_bounded, LossyChannel};
use vox_esp32_core::common::CoreError;
use vox_esp32_core::Controller;
use vox_esp32_power::metrics::{Metrics, VAId, VAType};
use vox_esp32_power::powmr_mppt::MPPTSummary;

static mut APP_CORE_STACK: esp_hal::system::Stack<65536> = esp_hal::system::Stack::new();

static APP_CORE_EXECUTOR: StaticCell<Executor> = StaticCell::new();

const BRIGHTNESS_OFF: u8 = 0;
const BRIGHTNESS_DIMMED: u8 = 20;
const BRIGHTNESS_ON: u8 = 70;

const SCREENSAVER_DIM_SCREEN_DELAY_MS: u64 = 30 * 1000; // 30 Seconds
const SCREENSAVER_ON_DELAY_MS: u64 = 5 * 60 * 1000; // 5 Minutes
const SCREENSAVER_CYCLE_DELAY_MS: u64 = 1 * 60 * 1000; // 1 Minute

#[embassy_executor::task]
async fn app_task(
    spawner: Spawner,
    app_update_rx: Receiver<AppSyncState>,
    lcd_peripherals: DisplayLCDPeripherals,
    touch_peripherals: DisplayTouchPeripherals,
) {
    log::info!("Starting app on core 1");

    let (window, ui) = ui::init().expect("Failed to init UI");

    let (brightness_tx, touched_rx) = display::spawn(
        &spawner,
        window,
        lcd_peripherals,
        touch_peripherals,
        BRIGHTNESS_ON,
    )
    .await
    .expect("Failed to spawn app task");

    let screensaver_cfg = AppScreenSaverCfg::new(
        SCREENSAVER_DIM_SCREEN_DELAY_MS,
        SCREENSAVER_ON_DELAY_MS,
        SCREENSAVER_CYCLE_DELAY_MS,
        BRIGHTNESS_OFF,
        BRIGHTNESS_ON,
        BRIGHTNESS_DIMMED,
    );

    let sync_mgr = AppSyncManager::new(
        ui,
        screensaver_cfg,
        app_update_rx,
        touched_rx,
        brightness_tx,
    );

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
    touched_rx: Receiver<TouchData>,
    screensaver_mgr: AppScreenSaverManager,
}

impl AppSyncManager {
    fn new(
        ui: AppWindow,
        screensaver_cfg: AppScreenSaverCfg,
        app_update_rx: Receiver<AppSyncState>,
        touched_rx: Receiver<TouchData>,
        set_brightness_tx: LossyChannel<u8>,
    ) -> Self {
        Self {
            ui,
            app_update_rx,
            touched_rx,
            screensaver_mgr: AppScreenSaverManager::new(screensaver_cfg, set_brightness_tx),
        }
    }

    async fn run(&mut self) {
        loop {
            let app_update_rx_fut = self.app_update_rx.recv().fuse();
            let touched_rx_fut = self.touched_rx.recv().fuse();
            let screensaver_timeout_fut =
                Timer::after(self.screensaver_mgr.update(&mut self.ui).await).fuse();

            pin_mut!(app_update_rx_fut, touched_rx_fut, screensaver_timeout_fut);

            select_biased! {
                msg_res = touched_rx_fut => {
                    match msg_res {
                        Ok(_) => {
                            if let Err(e) = self.screensaver_mgr.touched().await {
                                log::error!("Failed to trigger touched event for AppScreenSaverManager");
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to receive touch event from rx: {:?}", e);
                        }
                    }
                }
                msg_res = app_update_rx_fut => {
                    match msg_res {
                        Ok(update) => {
                            if let Err(e) = self.sync_app_state(update).await {
                                log::error!("Failed to sync app state: {:?}", e);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to receive app update from rx: {:?}", e);
                        }
                    }
                }
                _ = screensaver_timeout_fut => {
                    // Timeout (will process update at top of loop)
                }
            }
        }
    }

    async fn sync_app_state(&mut self, update: AppSyncState) -> Result<()> {
        let power_summary = update.power_metrics.summary();
        let batt_summary = power_summary.types.get(&VAType::Batt).ok_or(anyhow!(
            "Expected to have VAType::Batt in power metrics summary"
        ))?;
        let pv_summary = power_summary.types.get(&VAType::Pv).ok_or(anyhow!(
            "Expected to have VAType::Pv in power metrics summary"
        ))?;

        let power_state = ui::PowerState {
            va_entries: extract_power_va_entries(&update.power_metrics),
            batt_summary: batt_summary.into(),
            pv_summary: pv_summary.into(),
            mppt_summary: update.power_mppt.as_ref().into(),
        };

        self.ui
            .global::<ui::State>()
            .set_app_state(ui::AppState { power: power_state });

        Ok(())
    }
}

struct AppScreenSaverCfg {
    delay_dim: Duration,
    delay_on: Duration,
    delay_cycle: Duration,
    brightness_off: u8,
    brightness_on: u8,
    brightness_dimmed: u8,
}

impl AppScreenSaverCfg {
    fn new(
        delay_dim: u64,
        delay_on: u64,
        delay_cycle: u64,
        brightness_off: u8,
        brightness_on: u8,
        brightness_dimmed: u8,
    ) -> Self {
        Self {
            delay_dim: Duration::from_millis(delay_dim),
            delay_on: Duration::from_millis(delay_on),
            delay_cycle: Duration::from_millis(delay_cycle),
            brightness_off,
            brightness_on,
            brightness_dimmed,
        }
    }
}

struct AppScreenSaverManager {
    cfg: AppScreenSaverCfg,
    set_brightness_tx: LossyChannel<u8>,
    curent_brightness: u8,
    screensaver_on: bool,
    last_cycle: Instant,
    last_touch: Instant,
}

impl AppScreenSaverManager {
    fn new(cfg: AppScreenSaverCfg, set_brightness_tx: LossyChannel<u8>) -> Self {
        Self {
            cfg,
            set_brightness_tx,
            curent_brightness: 0, // Unset.
            screensaver_on: false,
            last_cycle: Instant::now(),
            last_touch: Instant::now(),
        }
    }

    async fn update(&mut self, ui: &mut AppWindow) -> Duration {
        let elapsed = Duration::from_micros(self.last_touch.elapsed().as_micros());
        if elapsed.ge(&self.cfg.delay_on) {
            // Screensaver on
            let mut elapsed_cycle = Duration::from_micros(self.last_cycle.elapsed().as_micros());
            if elapsed_cycle.ge(&self.cfg.delay_cycle) {
                self.cycle_screensaver(ui).await;
                elapsed_cycle = Duration::from_micros(self.last_cycle.elapsed().as_micros());
            }

            self.enable_screensaver(ui).await;
            self.set_brightness(self.cfg.brightness_dimmed).await;

            self.cfg.delay_cycle.sub(elapsed_cycle)
        } else if elapsed.ge(&self.cfg.delay_dim) {
            // Screen dimmed
            self.disable_screensaver(ui).await;
            self.set_brightness(self.cfg.brightness_dimmed).await;

            self.cfg.delay_on.sub(elapsed)
        } else {
            // Screen normal
            self.disable_screensaver(ui).await;
            self.set_brightness(self.cfg.brightness_on).await;

            self.cfg.delay_dim.sub(elapsed)
        }
    }

    async fn touched(&mut self) -> Result<()> {
        self.last_touch = Instant::now();

        Ok(())
    }

    async fn set_brightness(&mut self, brightness: u8) {
        if self.curent_brightness != brightness {
            if let Err(e) = self.set_brightness_tx.send_lossy(brightness).await {
                log::error!("Failed to send set brightness message: {:?}", e);
            } else {
                self.curent_brightness = brightness;
            }
        }
    }

    async fn enable_screensaver(&mut self, ui: &mut AppWindow) {
        if !ui.global::<ui::State>().get_screensaver_active() {
            ui.global::<ui::State>().set_screensaver_active(true);
            self.last_cycle = Instant::now();
        }
    }

    async fn cycle_screensaver(&mut self, ui: &mut AppWindow) {
        let active_theme = ui.global::<ui::State>().get_active_theme();

        let next_theme = match active_theme {
            ui::Theme::ShootingStars => ui::Theme::MagicKingdom,
            ui::Theme::MagicKingdom => ui::Theme::Verdant,
            ui::Theme::Verdant => ui::Theme::WhiteDeer,
            ui::Theme::WhiteDeer => ui::Theme::ShootingStars,
        };

        ui.global::<ui::State>().set_active_theme(next_theme);

        self.last_cycle = Instant::now();
    }

    async fn disable_screensaver(&mut self, ui: &mut AppWindow) {
        let cfg_theme = ui.global::<ui::State>().get_config_theme();
        if ui.global::<ui::State>().get_active_theme() != cfg_theme {
            ui.global::<ui::State>().set_active_theme(cfg_theme);
        }

        if ui.global::<ui::State>().get_screensaver_active() {
            ui.global::<ui::State>().set_screensaver_active(false);
        }
    }
}
