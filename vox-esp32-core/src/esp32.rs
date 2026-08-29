use esp_idf_hal::gpio::{
    Gpio0, Gpio1, Gpio10, Gpio11, Gpio12, Gpio13, Gpio14, Gpio15, Gpio16, Gpio17, Gpio18, Gpio19,
    Gpio2, Gpio20, Gpio21, Gpio26, Gpio27, Gpio28, Gpio29, Gpio3, Gpio30, Gpio31, Gpio32, Gpio33,
    Gpio34, Gpio35, Gpio36, Gpio37, Gpio38, Gpio39, Gpio4, Gpio40, Gpio41, Gpio42, Gpio43, Gpio44,
    Gpio45, Gpio46, Gpio47, Gpio48, Gpio5, Gpio6, Gpio7, Gpio8, Gpio9,
};
#[allow(deprecated)]
use esp_idf_hal::rmt::{
    CHANNEL0, CHANNEL1, CHANNEL2, CHANNEL3, CHANNEL4, CHANNEL5, CHANNEL6, CHANNEL7,
};
use esp_idf_hal::{adc, i2c, i2s, ledc, modem, rmt, spi, task::watchdog, uart, ulp, usb_serial};

pub struct Pins {
    pub gpio0: Option<Gpio0<'static>>,
    pub gpio1: Option<Gpio1<'static>>,
    pub gpio2: Option<Gpio2<'static>>,
    pub gpio3: Option<Gpio3<'static>>,
    pub gpio4: Option<Gpio4<'static>>,
    pub gpio5: Option<Gpio5<'static>>,
    pub gpio6: Option<Gpio6<'static>>,
    pub gpio7: Option<Gpio7<'static>>,
    pub gpio8: Option<Gpio8<'static>>,
    pub gpio9: Option<Gpio9<'static>>,
    pub gpio10: Option<Gpio10<'static>>,
    pub gpio11: Option<Gpio11<'static>>,
    pub gpio12: Option<Gpio12<'static>>,
    pub gpio13: Option<Gpio13<'static>>,
    pub gpio14: Option<Gpio14<'static>>,
    pub gpio15: Option<Gpio15<'static>>,
    pub gpio16: Option<Gpio16<'static>>,
    pub gpio17: Option<Gpio17<'static>>,
    pub gpio18: Option<Gpio18<'static>>,
    pub gpio19: Option<Gpio19<'static>>,
    pub gpio20: Option<Gpio20<'static>>,
    pub gpio21: Option<Gpio21<'static>>,
    pub gpio26: Option<Gpio26<'static>>,
    pub gpio27: Option<Gpio27<'static>>,
    pub gpio28: Option<Gpio28<'static>>,
    pub gpio29: Option<Gpio29<'static>>,
    pub gpio30: Option<Gpio30<'static>>,
    pub gpio31: Option<Gpio31<'static>>,
    pub gpio32: Option<Gpio32<'static>>,
    pub gpio33: Option<Gpio33<'static>>,
    pub gpio34: Option<Gpio34<'static>>,
    pub gpio35: Option<Gpio35<'static>>,
    pub gpio36: Option<Gpio36<'static>>,
    pub gpio37: Option<Gpio37<'static>>,
    pub gpio38: Option<Gpio38<'static>>,
    pub gpio39: Option<Gpio39<'static>>,
    pub gpio40: Option<Gpio40<'static>>,
    pub gpio41: Option<Gpio41<'static>>,
    pub gpio42: Option<Gpio42<'static>>,
    pub gpio43: Option<Gpio43<'static>>,
    pub gpio44: Option<Gpio44<'static>>,
    pub gpio45: Option<Gpio45<'static>>,
    pub gpio46: Option<Gpio46<'static>>,
    pub gpio47: Option<Gpio47<'static>>,
    pub gpio48: Option<Gpio48<'static>>,
}

impl Pins {
    pub(crate) fn new(pins: esp_idf_hal::gpio::Pins) -> Self {
        Self {
            gpio0: Some(pins.gpio0),
            gpio1: Some(pins.gpio1),
            gpio2: Some(pins.gpio2),
            gpio3: Some(pins.gpio3),
            gpio4: Some(pins.gpio4),
            gpio5: Some(pins.gpio5),
            gpio6: Some(pins.gpio6),
            gpio7: Some(pins.gpio7),
            gpio8: Some(pins.gpio8),
            gpio9: Some(pins.gpio9),
            gpio10: Some(pins.gpio10),
            gpio11: Some(pins.gpio11),
            gpio12: Some(pins.gpio12),
            gpio13: Some(pins.gpio13),
            gpio14: Some(pins.gpio14),
            gpio15: Some(pins.gpio15),
            gpio16: Some(pins.gpio16),
            gpio17: Some(pins.gpio17),
            gpio18: Some(pins.gpio18),
            gpio19: Some(pins.gpio19),
            gpio20: Some(pins.gpio20),
            gpio21: Some(pins.gpio21),
            gpio26: Some(pins.gpio26),
            gpio27: Some(pins.gpio27),
            gpio28: Some(pins.gpio28),
            gpio29: Some(pins.gpio29),
            gpio30: Some(pins.gpio30),
            gpio31: Some(pins.gpio31),
            gpio32: Some(pins.gpio32),
            gpio33: Some(pins.gpio33),
            gpio34: Some(pins.gpio34),
            gpio35: Some(pins.gpio35),
            gpio36: Some(pins.gpio36),
            gpio37: Some(pins.gpio37),
            gpio38: Some(pins.gpio38),
            gpio39: Some(pins.gpio39),
            gpio40: Some(pins.gpio40),
            gpio41: Some(pins.gpio41),
            gpio42: Some(pins.gpio42),
            gpio43: Some(pins.gpio43),
            gpio44: Some(pins.gpio44),
            gpio45: Some(pins.gpio45),
            gpio46: Some(pins.gpio46),
            gpio47: Some(pins.gpio47),
            gpio48: Some(pins.gpio48),
        }
    }
}

#[allow(deprecated)]
pub struct RMT {
    pub channel0: Option<CHANNEL0<'static>>,
    pub channel1: Option<CHANNEL1<'static>>,
    pub channel2: Option<CHANNEL2<'static>>,
    pub channel3: Option<CHANNEL3<'static>>,
    pub channel4: Option<CHANNEL4<'static>>,
    pub channel5: Option<CHANNEL5<'static>>,
    pub channel6: Option<CHANNEL6<'static>>,
    pub channel7: Option<CHANNEL7<'static>>,
}

#[allow(deprecated)]
impl RMT {
    pub fn new(r: rmt::RMT) -> Self {
        Self {
            channel0: Some(r.channel0),
            channel1: Some(r.channel1),
            channel2: Some(r.channel2),
            channel3: Some(r.channel3),
            channel4: Some(r.channel4),
            channel5: Some(r.channel5),
            channel6: Some(r.channel6),
            channel7: Some(r.channel7),
        }
    }
}

pub struct Peripherals {
    pub pins: Pins,
    pub uart0: Option<uart::UART0<'static>>,
    pub uart1: Option<uart::UART1<'static>>,
    pub uart2: Option<uart::UART2<'static>>,
    pub i2c0: Option<i2c::I2C0<'static>>,
    pub i2c1: Option<i2c::I2C1<'static>>,
    pub i2s0: Option<i2s::I2S0<'static>>,
    pub i2s1: Option<i2s::I2S1<'static>>,
    pub spi1: Option<spi::SPI1<'static>>,
    pub spi2: Option<spi::SPI2<'static>>,
    pub spi3: Option<spi::SPI3<'static>>,
    pub adc1: Option<adc::ADC1<'static>>,
    pub adc2: Option<adc::ADC2<'static>>,

    // Note: Kept legacy feature tags if your project relies on them
    #[cfg(feature = "pcnt-legacy")]
    pub pcnt0: Option<esp_idf_hal::pcnt::PCNT0<'static>>,
    #[cfg(feature = "pcnt-legacy")]
    pub pcnt1: Option<esp_idf_hal::pcnt::PCNT1<'static>>,
    #[cfg(feature = "pcnt-legacy")]
    pub pcnt2: Option<esp_idf_hal::pcnt::PCNT2<'static>>,
    #[cfg(feature = "pcnt-legacy")]
    pub pcnt3: Option<esp_idf_hal::pcnt::PCNT3<'static>>,

    pub can: Option<esp_idf_hal::can::CAN<'static>>,
    pub ledc: Option<ledc::LEDC>,

    #[cfg(feature = "rmt-legacy")]
    pub rmt: RMT,

    pub ulp: Option<ulp::ULP<'static>>,
    pub modem: Option<modem::Modem<'static>>,

    #[cfg(feature = "timer-legacy")]
    pub timer00: Option<esp_idf_hal::timer::TIMER00<'static>>,
    #[cfg(feature = "timer-legacy")]
    pub timer01: Option<esp_idf_hal::timer::TIMER01<'static>>,
    #[cfg(feature = "timer-legacy")]
    pub timer10: Option<esp_idf_hal::timer::TIMER10<'static>>,
    #[cfg(feature = "timer-legacy")]
    pub timer11: Option<esp_idf_hal::timer::TIMER11<'static>>,

    pub twdt: Option<watchdog::TWDT<'static>>,
    pub usb_serial: Option<usb_serial::USB_SERIAL<'static>>,
}

impl Peripherals {
    pub fn new(p: esp_idf_hal::peripherals::Peripherals) -> Self {
        Self {
            pins: Pins::new(p.pins),
            uart0: Some(p.uart0),
            uart1: Some(p.uart1),
            uart2: Some(p.uart2),
            i2c0: Some(p.i2c0),
            i2c1: Some(p.i2c1),
            i2s0: Some(p.i2s0),
            i2s1: Some(p.i2s1),
            spi1: Some(p.spi1),
            spi2: Some(p.spi2),
            spi3: Some(p.spi3),
            adc1: Some(p.adc1),
            adc2: Some(p.adc2),

            #[cfg(feature = "pcnt-legacy")]
            pcnt0: Some(p.pcnt0),
            #[cfg(feature = "pcnt-legacy")]
            pcnt1: Some(p.pcnt1),
            #[cfg(feature = "pcnt-legacy")]
            pcnt2: Some(p.pcnt2),
            #[cfg(feature = "pcnt-legacy")]
            pcnt3: Some(p.pcnt3),

            can: Some(p.can),
            ledc: Some(p.ledc),

            #[cfg(feature = "rmt-legacy")]
            rmt: RMT::new(p.rmt),

            ulp: Some(p.ulp),
            modem: Some(p.modem),

            #[cfg(feature = "timer-legacy")]
            timer00: Some(p.timer00),
            #[cfg(feature = "timer-legacy")]
            timer01: Some(p.timer01),
            #[cfg(feature = "timer-legacy")]
            timer10: Some(p.timer10),
            #[cfg(feature = "timer-legacy")]
            timer11: Some(p.timer11),

            twdt: Some(p.twdt),
            usb_serial: Some(p.usb_serial),
        }
    }
}
