use num_enum::TryFromPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u32)]
pub enum DeviceType {
    None = 1,
    Controller = 2,
    Power = 3,
    WaterHeater = 4,
}

impl DeviceType {
    pub fn id(&self) -> u32 {
        *self as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub device_type: DeviceType,
    pub device_id: u32,
}

impl Device {
    pub fn new(device_type: DeviceType, device_id: u32) -> Self {
        Self {
            device_id,
            device_type,
        }
    }
}
