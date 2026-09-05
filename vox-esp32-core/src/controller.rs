use anyhow::{anyhow, Result};

use esp_hal::clock::CpuClock;
//use esp_hal::gpio::Input;
//use esp_hal::gpio::Pull;
use crate::common::CoreError;
use esp_hal::timer::timg::TimerGroup;

use crate::esp32::Peripherals;
#[cfg(feature = "esp32s3-rgb-led")]
use crate::esp32_led::{LEDManager, LEDManagerHandle};

pub struct Controller {
    pub peripherals: Peripherals,
    //pub executor: LocalExecutor<'static>,
    // pub boot_button: PinDriver<'static, Input>,
    #[cfg(feature = "esp32s3-rgb-led")]
    pub led: LEDManagerHandle,
}

impl Controller {
    pub(crate) fn new(mut peripherals: Peripherals) -> Result<Self> {
        /*
        let boot_pin = peripherals.GPIO0.take().ok_or(anyhow!(
            "Controller: Failed to take boot pin, GPIO0 already taken"
        ))?;
        let boot_button = PinDriver::input(boot_pin, esp_idf_hal::gpio::Pull::Up)?;

        #[cfg(feature = "esp32s3-rgb-led")]
        let (led, led_task) = LEDManager::spawn(&mut executor, &mut peripherals)?;
        #[cfg(feature = "esp32s3-rgb-led")]
        tasks.push(led_task);

         */

        Ok(Self {
            peripherals,
            //executor,
            //boot_button,
            #[cfg(feature = "esp32s3-rgb-led")]
            led,
        })
    }

    pub(crate) fn setup() -> Result<Self> {
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

        Self::new(peripherals)
    }

    /*
    pub fn spawn<F>(&mut self, fut: F)
    where
        F: Future<Output = ()> + 'static,
    {
        self.tasks.push(self.executor.spawn(fut));
    }

     */
}
