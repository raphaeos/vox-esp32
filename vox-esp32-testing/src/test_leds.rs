use embassy_time::{Duration, Timer};
use vox_esp32_core::esp32_led::LEDColor;
use vox_esp32_core::Controller;

pub async fn run(controller: &mut Controller) -> ! {
    log::info!("Started LED test ...");

    let mut col_i = 0;
    let colors: [LEDColor; 6] = [
        LEDColor::Blue,
        LEDColor::Teal,
        LEDColor::Green,
        LEDColor::Purple,
        LEDColor::Orange,
        LEDColor::Pink,
    ];

    loop {
        controller.boot_button.wait_for_falling_edge().await;

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

        Timer::after(Duration::from_millis(200)).await;
    }
}
