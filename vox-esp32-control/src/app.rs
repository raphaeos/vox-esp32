use crate::display::{DisplayLCDPeripherals, DisplayTouchPeripherals};
use crate::{display, ui};
use core::ptr::addr_of_mut;
use embassy_executor::Spawner;
use esp_rtos::embassy::Executor;
use static_cell::StaticCell;
use vox_esp32_core::common::CoreError;
use vox_esp32_core::Controller;

static mut APP_CORE_STACK: esp_hal::system::Stack<65536> = esp_hal::system::Stack::new();

static APP_CORE_EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[embassy_executor::task]
async fn app_task(
    spawner: Spawner,
    lcd_peripherals: DisplayLCDPeripherals,
    touch_peripherals: DisplayTouchPeripherals,
) {
    log::info!("Starting app on core 1");

    let (window, _ui) = ui::init().expect("Failed to init UI");

    display::spawn(&spawner, window, lcd_peripherals, touch_peripherals)
        .await
        .expect("Failed to spawn app task");
}

pub async fn start(controller: &mut Controller) -> anyhow::Result<()> {
    let lcd_peripherals = DisplayLCDPeripherals::new(controller)?;
    let touch_peripherals = DisplayTouchPeripherals::new(controller)?;

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
            spawner.spawn(app_task(spawner, lcd_peripherals, touch_peripherals).unwrap());
        })
    });

    Ok(())
}
