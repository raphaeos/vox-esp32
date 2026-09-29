use crate::display::{UI_HEIGHT, UI_WIDTH};
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::ToString;
use anyhow::{anyhow, Result};
use slint::platform::software_renderer::MinimalSoftwareWindow;
use vox_esp32_power::metrics::{ProbeStatus, VAId, VAType, VAMetricEntry};

slint::include_modules!();

pub struct EspPlatform {
    pub window: Rc<MinimalSoftwareWindow>,
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

pub fn init() -> Result<(Rc<MinimalSoftwareWindow>, AppWindow)> {
    // A. Allocate the window instance and set the platform definition
    let window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::NewBuffer,
    );
    window.set_size(slint::PhysicalSize::new(UI_WIDTH as u32, UI_HEIGHT as u32));

    let platform = EspPlatform {
        window: window.clone(),
    };
    slint::platform::set_platform(Box::new(platform))?;

    // B. Instantiate your compiled UI component window
    let ui = AppWindow::new().map_err(|e| anyhow!(e))?;
    ui.show().map_err(|e| anyhow!(e))?;

    Ok((window.clone(), ui))
}

/// Type Mappings

impl From<&VAMetricEntry> for PowerVAEntry {
    fn from(value: &VAMetricEntry) -> Self {
        Self {
            name: value.id.to_string().into(),
            status: value.status.into(),
            r#type: value.id.into(),
            voltage: value.voltage,
            amperage: value.amperage,
        }
    }
}

impl From<ProbeStatus> for PowerVAStatus {
    fn from(value: ProbeStatus) -> Self {
        match value {
            ProbeStatus::Init => PowerVAStatus::Init,
            ProbeStatus::Ok => PowerVAStatus::Ok,
            ProbeStatus::Err(_) => PowerVAStatus::Error,
        }
    }
}

impl From<VAId> for PowerVAType {
    fn from(value: VAId) -> Self {
        value.va_type().into()
    }
}

impl From<VAType> for PowerVAType {
    fn from(value: VAType) -> Self {
        match value {
            VAType::Batt => PowerVAType::Batt,
            VAType::Pv => PowerVAType::Pv,
        }
    }
}