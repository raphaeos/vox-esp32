use crate::common::CoreError;
use anyhow::{anyhow, Result};
use embassy_executor::{SpawnToken, Spawner};
use esp_hal::clock::CpuClock;
use esp_hal::gpio;
use esp_hal::gpio::InputConfig;
use esp_hal::timer::timg::TimerGroup;

use crate::esp32::Peripherals;
use crate::esp32_can::{CANManager, CANManagerHandle, CANRxHandler};
#[cfg(feature = "esp32s3-rgb-led")]
use crate::esp32_led::{LEDManager, LEDManagerHandle};
use crate::types::Device;

extern crate alloc;

pub struct Controller {
    pub device: Device,
    pub peripherals: Peripherals,
    pub boot_button: gpio::Input<'static>,
    #[cfg(feature = "esp32s3-rgb-led")]
    pub led: LEDManagerHandle,
    spawner: Spawner,
    #[cfg(feature = "esp32s3-can")]
    can_mgr: Option<CANManager>,
}

impl Controller {
    pub(crate) fn new(
        device: Device,
        spawner: Spawner,
        mut peripherals: Peripherals,
    ) -> Result<Self> {
        let boot_config = InputConfig::default().with_pull(gpio::Pull::Up);

        let boot_button = gpio::Input::new(
            peripherals
                .GPIO0
                .take()
                .ok_or(CoreError::PeripheralTaken("GPIO0"))?,
            boot_config,
        );

        #[cfg(feature = "esp32s3-rgb-led")]
        let led = LEDManager::spawn(&spawner, &mut peripherals)?;

        #[cfg(feature = "esp32s3-can")]
        let can_mgr = Some(CANManager::new());

        Ok(Self {
            device,
            spawner,
            peripherals,
            boot_button,
            #[cfg(feature = "esp32s3-rgb-led")]
            led,
            #[cfg(feature = "esp32s3-can")]
            can_mgr,
        })
    }

    pub(crate) fn setup(device: Device, spawner: Spawner) -> Result<Self> {
        let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
        let mut peripherals = Peripherals::new(esp_hal::init(config));

        esp_alloc::heap_allocator!(
            #[esp_hal::ram(reclaimed)]
            size: 72 * 1024
        );

        let timg0 = TimerGroup::new(
            peripherals
                .TIMG0
                .take()
                .ok_or(CoreError::PeripheralTaken("TIMG0"))?,
        );

        esp_rtos::start(
            timg0.timer0,
            peripherals
                .FROM_CPU_INTR0
                .take()
                .ok_or(CoreError::PeripheralTaken("FROM_CPU_INTR0"))?,
        );

        Self::new(device, spawner, peripherals)
    }

    pub fn spawn<S>(&self, token: SpawnToken<S>) {
        self.spawner.spawn(token);
    }

    pub fn add_can_handler(&mut self, handler: CANRxHandler) -> Result<&mut Self> {
        if let Some(can_mgr) = self.can_mgr.as_mut() {
            can_mgr.add_handler(handler);
        } else {
            return Err(anyhow!(
                "Failed to add can handler: can_mgr has already been taken!"
            ));
        }

        Ok(self)
    }

    pub fn start_can(&mut self) -> Result<CANManagerHandle> {
        if let Some(can_mgr) = self.can_mgr.take() {
            can_mgr.spawn(self)
        } else {
            Err(anyhow!(
                "Failed to start CAN: can_mgr has already been taken!"
            ))
        }
    }
}
