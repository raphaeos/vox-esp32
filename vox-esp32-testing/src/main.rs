use anyhow::Result;
use embassy_time::Duration;
use vox_esp32_core::esp32_led::{LEDColor, LEDStatus};

fn main() -> Result<()> {
    let mut controller = vox_esp32_core::init()?;

    edge_executor::block_on(controller.executor.run(async {
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

                col_i += 1;

                // Extra pause to avoid dupe presses.
                embassy_time::Timer::after(Duration::from_millis(200)).await;
            }

            embassy_time::Timer::after(Duration::from_millis(20)).await;
        }
    }));

    Ok(())
}

/*
async fn run(controller: &mut Controller) -> Result<()> {
    let mut last_button_state = false;

    // 5. Main Application Thread Loop
    loop {
        let is_pressed = controller.boot_button.is_low();

        // Detect a clean falling edge button press
        if is_pressed && !last_button_state {
            log::info!("Button Press Detected! Overriding background blink safely.");

            controller.set_led_status(LEDColor::Green, LEDStatus::On)?;
        }
        // Reset state when button is released
        else if !is_pressed && last_button_state {
            log::info!("Button Released! Re-engaging background blink.");

            controller.set_led_status(LEDColor::Purple, LEDStatus::Blink)?;
        }

        last_button_state = is_pressed;

        // Yield control back to the executor so the blink loop can run
        embassy_time::Timer::after(Duration::from_millis(20)).await;
    }

    Ok(())
}
*/
