#![no_std]

use anyhow::Result;
use embassy_time::Duration;
use vox_esp32_core::esp32_led::{LEDColor, LEDStatus};

fn main() -> Result<()> {
    let mut controller = vox_esp32_core::init()?;

    let main_future = Box::pin(async {
        controller
            .led
            .set(Some(LEDColor::Purple), Some(LEDStatus::Blink), None, None)
            .await;

        let mut col_i = 0;
        let colors: Vec<LEDColor> = vec![
            LEDColor::Blue,
            LEDColor::Teal,
            LEDColor::Green,
            LEDColor::Purple,
            LEDColor::Orange,
            LEDColor::Pink,
        ];

        loop {
            if controller.boot_button.wait_for_falling_edge().await.is_ok() {
                if col_i >= colors.len() {
                    col_i = 0;
                }

                let selected_color = colors[col_i];

                log::info!("Selected: {:?}", selected_color);

                controller
                    .led
                    .set(Some(selected_color), None, None, None)
                    .await;

                controller
                    .led
                    .once(
                        selected_color,
                        Some(255),
                        Some(Duration::from_millis(1000)),
                        None,
                    )
                    .await;

                col_i += 1;

                // Extra pause to avoid dupe presses.
                embassy_time::Timer::after(Duration::from_millis(200)).await;
            }

            embassy_time::Timer::after(Duration::from_millis(20)).await;
        }
    });

    edge_executor::block_on(controller.executor.run(main_future));

    Ok(())
}
