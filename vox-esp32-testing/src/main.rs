use anyhow::Result;
use embassy_time::Duration;
use vox_esp32_core::controller::{Controller, LEDColor, LEDStatus};

fn main() -> Result<()> {
    let mut controller = vox_esp32_core::init()?;

    //controller.set_led_status(LEDColor::Purple, LEDStatus::On)?;

    //controller.set_led_status(LEDColor::Blue, LEDStatus::Blink)?;

    edge_executor::block_on(controller.executor.run(async {
        //if let Err(e) = run(&mut controller).await {
        //    log::error!("Failed to run: {}", e);
        //}

        loop {
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
