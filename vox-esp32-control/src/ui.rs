use alloc::boxed::Box;
use alloc::rc::Rc;
use anyhow::{anyhow, Result};
use slint::platform::software_renderer::MinimalSoftwareWindow;

slint::include_modules!();

struct EspPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl slint::platform::Platform for EspPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }

    // Connect Slint directly to your Embassy timing engine
    fn duration_since_start(&self) -> core::time::Duration {
        core::time::Duration::from_millis(embassy_time::Instant::now().as_millis())
    }
}

pub fn init() -> Result<()> {
    // A. Allocate the window instance and set the platform definition
    let window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::NewBuffer,
    );
    window.set_size(slint::PhysicalSize::new(320, 480));

    let platform = EspPlatform {
        window: window.clone(),
    };
    slint::platform::set_platform(Box::new(platform))?;

    // B. Instantiate your compiled UI component window
    let ui = AppWindow::new().map_err(|e| anyhow!(e))?;
    ui.show().map_err(|e| anyhow!(e))?;

    Ok(())
}
