use alloc::format;
use alloc::string::String;
use anyhow::Result;
use async_channel::{bounded, Receiver, Sender};
use core::fmt::{Display, Formatter};
use embassy_time::Duration;
use esp_hal::uart::RxError;
use esp_hal::{
    time::Rate,
    uart::{Config, Uart},
    Async,
};
use num_enum::TryFromPrimitive;
use thiserror::Error;
use vox_esp32_core::common::CoreError;
use vox_esp32_core::esp32_led::{LEDManager, LEDManagerHandle, LED};
use vox_esp32_core::Controller;

const MPPT_FRAME_TYPE_USE: u8 = 0x0d;

#[derive(Error, Debug)]
pub enum MPPTError {
    #[error("Timed-out waiting for heartbeat from MPPT parallel communications")]
    TimeOut,
    #[error("Read error: {0}")]
    ReadError(#[from] RxError),
    #[error("Non-sync frame from MPPT parallel communications")]
    NonSyncFrame,
    #[error("CRC Miss-match in MPPT parallel communications")]
    CrcMissMatch,
    #[error("Unknown battery type idx: {0}")]
    UnknownBatteryTypeIdx(u8),
    #[error("Error: {0}")]
    Other(#[from] anyhow::Error),
}

// Define your custom strict Result type
pub type MPPTResult<T> = Result<T, MPPTError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum BatteryType {
    SEL = 0,
    GEL = 1,
    Fld = 2,
    L04 = 3,
    L07 = 4,
    L08 = 5,
    L15 = 6,
    L16 = 7,
    N03 = 8,
    N06 = 9,
    N07 = 10,
    N13 = 11,
    N14 = 12,
    USE = 13,
}

impl BatteryType {
    pub fn name(&self) -> &str {
        match self {
            BatteryType::SEL => "SEL (Lead-acid)",
            BatteryType::GEL => "GEL",
            BatteryType::Fld => "FLd (Flooded)",
            BatteryType::L04 => "L04 (4S LiFePO4)",
            BatteryType::L07 => "L07 (7S LiFePO4)",
            BatteryType::L08 => "L08 (8S LiFePO4)",
            BatteryType::L15 => "L15 (15S LiFePO4)",
            BatteryType::L16 => "L16 (16S LiFePO4)",
            BatteryType::N03 => "n03",
            BatteryType::N06 => "n06",
            BatteryType::N07 => "n07",
            BatteryType::N13 => "n13",
            BatteryType::N14 => "n14",
            BatteryType::USE => "USE (User Defined)",
        }
    }
}

pub struct MPPTUserConfig {
    pub boost_voltage: u16,
    pub float_voltage: u16,
    pub uv_cutoff: u16,
    pub uv_recovery: u16,
    // I'm not sure it's calibration or like initial voltage, it doesn't look like calibration.
    pub calibration_voltage: u16,
    pub max_current: u16,
}

impl MPPTUserConfig {
    pub fn boost_voltage(&self) -> f32 {
        self.boost_voltage as f32 / 10.0
    }

    pub fn float_voltage(&self) -> f32 {
        self.float_voltage as f32 / 10.0
    }

    pub fn uv_cutoff(&self) -> f32 {
        self.uv_cutoff as f32 / 10.0
    }

    pub fn uv_recovery(&self) -> f32 {
        self.uv_recovery as f32 / 10.0
    }

    pub fn calibration_voltage(&self) -> f32 {
        self.calibration_voltage as f32 / 10.0
    }

    pub fn max_current(&self) -> f32 {
        self.max_current as f32 / 100.0
    }
}

impl Display for MPPTUserConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "MPPTUserConfig[boost_voltage: {:.1} V, float_voltage: {:.1} V, uv_cutoff: {:.1} V, uv_recovery: {:.1} V, calibration_voltage: {:.1} V, max_current: {:.1} A]",
               self.boost_voltage(), self.float_voltage(), self.uv_cutoff(), self.uv_recovery(), self.calibration_voltage(), self.max_current())
    }
}

pub struct MPPTState {
    pub master_id: u8,
    pub battery_voltage: u16,
    pub battery_type: BatteryType,
    pub user_config: Option<MPPTUserConfig>,
}

impl MPPTState {
    pub fn battery_voltage(&self) -> f32 {
        self.battery_voltage as f32 / 10.0
    }

    pub fn soc(&self) -> u8 {
        battery_voltage_soc(self.battery_voltage())
    }
}

impl Display for MPPTState {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        if let Some(user_config) = self.user_config.as_ref() {
            write!(f, "MPPTState[master_id: {}, battery_voltage: {:.1} V, SOC: {} %, battery_type: {}, user_config: Some({})]",
                   self.master_id, self.battery_voltage(), self.soc(), self.battery_type.name(), user_config)
        } else {
            write!(f, "MPPTState[master_id: {}, battery_voltage: {:.1} V, SOC: {} %, battery_type: {}, user_config: None]",
                   self.master_id, self.battery_voltage(), self.soc(), self.battery_type.name())
        }
    }
}

#[embassy_executor::task]
async fn mppt_task(mut mgr: MPPTManager) {
    mgr.run().await;
}

pub struct MPPTManager {
    uart: Uart<'static, Async>,
    tx: Sender<MPPTResult<MPPTState>>,
    led: LEDManagerHandle,
}

impl MPPTManager {
    pub(crate) fn spawn(controller: &mut Controller) -> Result<Receiver<MPPTResult<MPPTState>>> {
        let config = Config::default().with_baudrate(9600);

        let mut uart = Uart::new(
            controller
                .peripherals
                .UART1
                .take()
                .ok_or(CoreError::PeripheralTaken("UART1"))?,
            config,
        )?
        .with_rx(
            controller
                .peripherals
                .GPIO18
                .take()
                .ok_or(CoreError::PeripheralTaken("GPIO18"))?,
        )
        .into_async();

        let (tx, rx) = bounded(10);

        let led = controller.led.clone();

        let mgr = MPPTManager { uart, tx, led };

        controller.spawn(mppt_task(mgr)?);

        Ok(rx)
    }

    async fn run(&mut self) {
        log::info!("Vox ESP32 Solar: PowMr MPPT Manager started");

        loop {
            log::debug!("MPPTManager: Waiting for parallel communications ...");

            let mut buf = [0u8; 64];

            match embassy_time::with_timeout(
                Duration::from_millis(20000),
                self.uart.read_async(&mut buf),
            )
            .await
            {
                Ok(Ok(total_read)) => {
                    log::debug!("MPPTManager: ... Read OK, read={}", total_read);

                    let data = &buf[..total_read];

                    if data[0] != 0x55 {
                        log::warn!(
                            "MPPTManager: Non-sync frame ({} bytes): {}",
                            total_read,
                            data.iter()
                                .map(|b| format!("{:02x} ", b))
                                .collect::<String>()
                        );

                        let _ = self.tx.send(Err(MPPTError::NonSyncFrame)).await;

                        embassy_time::Timer::after(Duration::from_millis(3000)).await;
                        continue;
                    }

                    log::debug!(
                        "MPPTManager: ... RAW ({}B): {}",
                        total_read,
                        data.iter()
                            .map(|b| format!("{:02x} ", b))
                            .collect::<String>()
                    );

                    let master_id = data[1];
                    let frame_type = data[2];
                    // Unknown really.
                    //let temp_or_soc = data[3];
                    let battery_voltage = ((data[4] as u16) << 8) | data[5] as u16;
                    //let volt_v = volt_raw as f32 / 10.0;
                    let batt_idx = data[6];
                    // Don't really care since we're just reading up to 64.
                    //let frame_length = data[7];

                    let crc_end = if frame_type == MPPT_FRAME_TYPE_USE {
                        21
                    } else {
                        8
                    };
                    let crc_rx = ((data[crc_end] as u16) << 8) | data[crc_end + 1] as u16;
                    let crc_calc = crc16_modbus(&data[0..crc_end]);

                    if crc_calc != crc_rx {
                        log::warn!(
                            "MPPTManager: CRC miss-match during parallel communications ({} vs {})",
                            crc_calc,
                            crc_rx
                        );

                        let _ = self.tx.send(Err(MPPTError::CrcMissMatch)).await;

                        continue;
                    }

                    match BatteryType::try_from(batt_idx) {
                        Ok(battery_type) => {
                            if frame_type == MPPT_FRAME_TYPE_USE && total_read >= 24 {
                                let boost_voltage = ((data[9] as u16) << 8) | data[10] as u16;
                                let float_voltage = ((data[11] as u16) << 8) | data[12] as u16;
                                let uv_cutoff = ((data[13] as u16) << 8) | data[14] as u16;
                                let uv_recovery = ((data[15] as u16) << 8) | data[16] as u16;
                                let calibration_voltage =
                                    ((data[17] as u16) << 8) | data[18] as u16;
                                let max_current = ((data[19] as u16) << 8) | data[20] as u16;

                                let _ = self
                                    .tx
                                    .send(Ok(MPPTState {
                                        master_id,
                                        battery_voltage,
                                        battery_type,
                                        user_config: Some(MPPTUserConfig {
                                            boost_voltage,
                                            float_voltage,
                                            uv_cutoff,
                                            uv_recovery,
                                            calibration_voltage,
                                            max_current,
                                        }),
                                    }))
                                    .await;
                            } else {
                                let _ = self
                                    .tx
                                    .send(Ok(MPPTState {
                                        master_id,
                                        battery_voltage,
                                        battery_type,
                                        user_config: None,
                                    }))
                                    .await;
                            }
                        }
                        Err(_e) => {
                            log::warn!("MPPTManager: Unknown battery type idx: {}", batt_idx);

                            let _ = self
                                .tx
                                .send(Err(MPPTError::UnknownBatteryTypeIdx(batt_idx)))
                                .await;
                        }
                    }
                }
                Ok(Err(err)) => {
                    log::warn!("MPPTManager: Read error: {:?}", err);

                    let _ = self.tx.send(Err(MPPTError::ReadError(err))).await;
                }
                Err(_err) => {
                    log::warn!("MPPTManager: Timed out waiting for parallel communications");

                    let _ = self.tx.send(Err(MPPTError::TimeOut)).await;
                }
            }
        }
    }
}

/// Utils

pub fn battery_voltage_soc(voltage: f32) -> u8 {
    if voltage >= 14.2 {
        100
    }
    // Full charge (3.55V/cell)
    else if voltage >= 14.0 {
        90
    }
    // Top of bulk
    else if voltage >= 13.6 {
        75
    }
    // Entering flat zone
    else if voltage >= 13.2 {
        50
    }
    // Middle of flat zone
    else if voltage >= 12.8 {
        25
    }
    // Lower flat zone
    else if voltage >= 12.4 {
        10
    }
    // Leaving flat zone
    else if voltage >= 12.0 {
        5
    }
    // Steep drop begins
    else if voltage >= 11.0 {
        1
    }
    // Barely hanging on
    else {
        0
    } // BMS cut-off imminent
}

// CRC-16/MODBUS (poly 0xA001 reflected, init 0xFFFF).
fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}
