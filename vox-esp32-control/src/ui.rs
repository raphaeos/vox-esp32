use crate::display::{UI_HEIGHT, UI_WIDTH};
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::ToString;
use anyhow::{anyhow, Result};
use slint::{ModelRc, SharedString, platform::software_renderer::MinimalSoftwareWindow};
use vox_esp32_power::{metrics::{
    Metrics, ProbeStatus, SummaryMetricEntry, VAId, VAMetricEntry, VAType, calculate_milliwatts, format_amperage, format_voltage,
}, powmr_mppt::{BatteryType, MPPTSummary, MPPTSummaryStatus}};

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

impl From<&Metrics> for ModelRc<PowerVAEntry> {
    fn from(value: &Metrics) -> Self {
        let mut va_entries: Vec<ui::PowerVAEntry> = Vec::new();
        // Iterate enum to preserve order.
        for va_id in VAId::iter() {
            if let Some(entry) = value.va_entries.get(&va_id) {
                va_entries.push(entry.into());
            }
        }

        slint::ModelRc::from(&va_entries[..])
    }
}

impl From<&VAMetricEntry> for PowerVAEntry {
    fn from(value: &VAMetricEntry) -> Self {
        Self {
            name: value.id.to_string().into(),
            status: value.status.into(),
            r#type: value.id.into(),
            voltage: value.voltage,
            voltage_text: value.format_voltage().into(),
            amperage: value.amperage,
            amperage_text: value.format_amperage().into(),
            wattage: value.wattage,
            wattage_text: value.format_wattage().into(),
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

impl From<&SummaryMetricEntry> for PowerTypeSummary {
    fn from(value: &SummaryMetricEntry) -> Self {
        Self {
            name: value.r#type.to_string().into(),
            r#type: value.r#type.into(),
            status: value.status.into(),
            amperage: value.amperage,
            amperage_text: value.format_amperage().into(),
            voltage: value.voltage,
            voltage_text: value.format_voltage().into(),
            wattage: value.wattage,
            wattage_text: value.format_wattage().into(),
        }
    }
}

impl From<SummaryMetricStatus> for PowerTypeStatus {
    fn from(value: SummaryMetricStatus) -> Self {
        match value {
            SummaryMetricStatus::Init => Self::Init,
            SummaryMetricStatus::Ok => Self::Ok,
            SummaryMetricStatus::Degraded => Self::Degraded,
            SummaryMetricStatus::Error => Self::Error,
        }
    }
}

impl From<&MPPTSummary> for PowerMPPTSummary {
    fn from(value: &MPPTSummary) -> Self {
        let mut boost_voltage: f32 = 0.0;
        let mut boost_voltage_text: SharedString = "".into();
        if let (Some(v), Some(vt)) = (value.boost_voltage(), value.format_boost_voltage()) {
            boost_voltage = v;
            boost_voltage_text = vt.into();
        }

        let mut float_voltage: f32 = 0.0;
        let mut float_voltage_text: SharedString = "".into();
        if let (Some(v), Some(vt)) = (value.float_voltage(), value.format_float_voltage()) {
            float_voltage = v;
            float_voltage_text = vt.into();
        }

        let mut battery_type_text: SharedString = "".into();
        if let MPPTSummaryStatus::Ok(battery_type) = value.status {
            battery_type_text = battery_type.name().into();
        }

        Self {
            status: value.status.into(),
            master_id: value.master_id as i32,
            battery_type: value.status.into(),
            battery_type_text,
            battery_soc: value.soc() as i32,
            battery_voltage: value.battery_voltage(),
            battery_voltage_text: value.format_battery_voltage().into(),
            boost_voltage,
            boost_voltage_text,
            float_voltage,
            float_voltage_text
        }
    }
}

impl From<&MPPTSummaryStatus> for PowerMPPTStatus {
    fn from(value: &MPPTSummaryStatus) -> Self {
        match value {
            MPPTSummaryStatus::Init => PowerMPPTStatus::Init,
            MPPTSummaryStatus::Err(_) => PowerMPPTStatus::Error,
            MPPTSummaryStatus::Ok(_) => PowerMPPTStatus::Ok,
        }
    }
}

impl From<&MPPTSummaryStatus> for PowerMPPTBatteryType {
    fn from(value: &MPPTSummaryStatus) -> Self {
        if let MPPTSummaryStatus::Ok(battery_type) = value.status {
            match battery_type {
                BatteryType::SEL => PowerMPPTBatteryType::SEL,
                BatteryType::GEL => PowerMPPTBatteryType::GEL,
                BatteryType::Fld => PowerMPPTBatteryType::Fld,
                BatteryType::L04 => PowerMPPTBatteryType::L04,
                BatteryType::L07 => PowerMPPTBatteryType::L07,
                BatteryType::L08 => PowerMPPTBatteryType::L08,
                BatteryType::L15 => PowerMPPTBatteryType::L15,
                BatteryType::L16 => PowerMPPTBatteryType::L16,
                BatteryType::N03 => PowerMPPTBatteryType::N03,
                BatteryType::N06 => PowerMPPTBatteryType::N06,
                BatteryType::N07 => PowerMPPTBatteryType::N07,
                BatteryType::N13 => PowerMPPTBatteryType::N13,
                BatteryType::N14 => PowerMPPTBatteryType::N14,
                BatteryType::USE => PowerMPPTBatteryType::USE,
            }
        } else {
            PowerMPPTBatteryType::None
        }
    }
}