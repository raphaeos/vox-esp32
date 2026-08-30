use anyhow::{anyhow, Result};
use edge_executor::{LocalExecutor, Task};
use esp_idf_hal::gpio::{Input, PinDriver};
use std::future::Future;

use crate::esp32::Peripherals;
use crate::esp32_led::{LEDManager, LEDManagerHandle, LED};

pub struct Controller {
    pub peripherals: Peripherals,
    pub executor: LocalExecutor<'static>,
    pub boot_button: PinDriver<'static, Input>,
    #[cfg(feature = "esp32s3-rgb-led")]
    pub led: LEDManagerHandle,
    #[allow(unused)]
    tasks: Vec<Task<()>>,
}

impl Controller {
    pub(crate) fn new(mut peripherals: Peripherals) -> Result<Self> {
        let mut executor = LocalExecutor::new();
        let mut tasks: Vec<Task<()>> = Vec::new();

        let boot_pin = peripherals.pins.gpio0.take().ok_or(anyhow!(
            "Controller: Failed to take boot pin, GPIO0 already taken"
        ))?;
        let boot_button = PinDriver::input(boot_pin, esp_idf_hal::gpio::Pull::Up)?;

        #[cfg(feature = "esp32s3-rgb-led")]
        let (led, led_task) = LEDManager::spawn(&mut executor, &mut peripherals)?;
        #[cfg(feature = "esp32s3-rgb-led")]
        tasks.push(led_task);

        Ok(Self {
            peripherals,
            executor,
            boot_button,
            #[cfg(feature = "esp32s3-rgb-led")]
            led,
            tasks,
        })
    }

    pub(crate) fn setup() -> Result<Self> {
        Self::new(Peripherals::new(
            esp_idf_hal::peripherals::Peripherals::take()?,
        ))
    }

    pub fn spawn<F>(&mut self, fut: F)
    where
        F: Future<Output = ()> + 'static,
    {
        self.tasks.push(self.executor.spawn(fut));
    }
}
