use crate::common::CoreError;
use anyhow::Result;
use embassy_executor::{SpawnToken, Spawner};
use esp_hal::clock::CpuClock;
use esp_hal::gpio;
use esp_hal::gpio::InputConfig;
use esp_hal::timer::timg::TimerGroup;

use crate::esp32::Peripherals;
#[cfg(feature = "esp32s3-rgb-led")]
use crate::esp32_led::{LEDManager, LEDManagerHandle};

extern crate alloc;

pub struct Controller {
    spawner: Spawner,
    pub peripherals: Peripherals,
    pub boot_button: gpio::Input<'static>,
    #[cfg(feature = "esp32s3-rgb-led")]
    pub led: LEDManagerHandle,
}

impl Controller {
    pub(crate) fn new(spawner: Spawner, mut peripherals: Peripherals) -> Result<Self> {
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

        Ok(Self {
            spawner,
            peripherals,
            boot_button,
            #[cfg(feature = "esp32s3-rgb-led")]
            led,
        })
    }

    pub(crate) fn setup(spawner: Spawner) -> Result<Self> {
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

        Self::new(spawner, peripherals)
    }

    pub fn spawn<S>(&self, token: SpawnToken<S>) {
        self.spawner.spawn(token);
    }
}
