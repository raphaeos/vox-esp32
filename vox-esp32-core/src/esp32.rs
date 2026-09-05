use esp_hal::peripherals::{
    ADC1, ADC2, FROM_CPU_INTR0, GPIO0, GPIO1, GPIO10, GPIO11, GPIO12, GPIO13, GPIO14, GPIO15,
    GPIO16, GPIO17, GPIO18, GPIO19, GPIO2, GPIO20, GPIO21, GPIO26, GPIO27, GPIO28, GPIO29, GPIO3,
    GPIO30, GPIO31, GPIO32, GPIO33, GPIO34, GPIO35, GPIO36, GPIO37, GPIO38, GPIO39, GPIO4, GPIO40,
    GPIO41, GPIO42, GPIO43, GPIO44, GPIO45, GPIO46, GPIO47, GPIO48, GPIO5, GPIO6, GPIO7, GPIO8,
    GPIO9, I2C0, I2C1, I2S0, I2S1, LEDC, PCNT, RMT, SPI0, SPI1, SPI2, TIMG0, TIMG1, TWAI0, UART0,
    UART1, UART2,
};

#[allow(non_snake_case)]
pub struct Peripherals {
    pub GPIO0: Option<GPIO0<'static>>,
    pub GPIO1: Option<GPIO1<'static>>,
    pub GPIO2: Option<GPIO2<'static>>,
    pub GPIO3: Option<GPIO3<'static>>,
    pub GPIO4: Option<GPIO4<'static>>,
    pub GPIO5: Option<GPIO5<'static>>,
    pub GPIO6: Option<GPIO6<'static>>,
    pub GPIO7: Option<GPIO7<'static>>,
    pub GPIO8: Option<GPIO8<'static>>,
    pub GPIO9: Option<GPIO9<'static>>,
    pub GPIO10: Option<GPIO10<'static>>,
    pub GPIO11: Option<GPIO11<'static>>,
    pub GPIO12: Option<GPIO12<'static>>,
    pub GPIO13: Option<GPIO13<'static>>,
    pub GPIO14: Option<GPIO14<'static>>,
    pub GPIO15: Option<GPIO15<'static>>,
    pub GPIO16: Option<GPIO16<'static>>,
    pub GPIO17: Option<GPIO17<'static>>,
    pub GPIO18: Option<GPIO18<'static>>,
    pub GPIO19: Option<GPIO19<'static>>,
    pub GPIO20: Option<GPIO20<'static>>,
    pub GPIO21: Option<GPIO21<'static>>,
    pub GPIO26: Option<GPIO26<'static>>,
    pub GPIO27: Option<GPIO27<'static>>,
    pub GPIO28: Option<GPIO28<'static>>,
    pub GPIO29: Option<GPIO29<'static>>,
    pub GPIO30: Option<GPIO30<'static>>,
    pub GPIO31: Option<GPIO31<'static>>,
    pub GPIO32: Option<GPIO32<'static>>,
    pub GPIO33: Option<GPIO33<'static>>,
    pub GPIO34: Option<GPIO34<'static>>,
    pub GPIO35: Option<GPIO35<'static>>,
    pub GPIO36: Option<GPIO36<'static>>,
    pub GPIO37: Option<GPIO37<'static>>,
    pub GPIO38: Option<GPIO38<'static>>,
    pub GPIO39: Option<GPIO39<'static>>,
    pub GPIO40: Option<GPIO40<'static>>,
    pub GPIO41: Option<GPIO41<'static>>,
    pub GPIO42: Option<GPIO42<'static>>,
    pub GPIO43: Option<GPIO43<'static>>,
    pub GPIO44: Option<GPIO44<'static>>,
    pub GPIO45: Option<GPIO45<'static>>,
    pub GPIO46: Option<GPIO46<'static>>,
    pub GPIO47: Option<GPIO47<'static>>,
    pub GPIO48: Option<GPIO48<'static>>,

    pub UART0: Option<UART0<'static>>,
    pub UART1: Option<UART1<'static>>,
    pub UART2: Option<UART2<'static>>,

    pub I2C0: Option<I2C0<'static>>,
    pub I2C1: Option<I2C1<'static>>,

    pub I2S0: Option<I2S0<'static>>,
    pub I2S1: Option<I2S1<'static>>,

    pub SPI0: Option<SPI0<'static>>,
    pub SPI1: Option<SPI1<'static>>,
    pub SPI2: Option<SPI2<'static>>,

    pub LEDC: Option<LEDC<'static>>,

    pub TWAI0: Option<TWAI0<'static>>,

    pub PCNT: Option<PCNT<'static>>,

    pub RMT: Option<RMT<'static>>,

    pub TIMG0: Option<TIMG0<'static>>,
    pub TIMG1: Option<TIMG1<'static>>,

    pub ADC1: Option<ADC1<'static>>,
    pub ADC2: Option<ADC2<'static>>,

    pub FROM_CPU_INTR0: Option<FROM_CPU_INTR0<'static>>,
}

impl Peripherals {
    pub fn new(p: esp_hal::peripherals::Peripherals) -> Self {
        Self {
            GPIO0: Some(p.GPIO0),
            GPIO1: Some(p.GPIO1),
            GPIO2: Some(p.GPIO2),
            GPIO3: Some(p.GPIO3),
            GPIO4: Some(p.GPIO4),
            GPIO5: Some(p.GPIO5),
            GPIO6: Some(p.GPIO6),
            GPIO7: Some(p.GPIO7),
            GPIO8: Some(p.GPIO8),
            GPIO9: Some(p.GPIO9),
            GPIO10: Some(p.GPIO10),
            GPIO11: Some(p.GPIO11),
            GPIO12: Some(p.GPIO12),
            GPIO13: Some(p.GPIO13),
            GPIO14: Some(p.GPIO14),
            GPIO15: Some(p.GPIO15),
            GPIO16: Some(p.GPIO16),
            GPIO17: Some(p.GPIO17),
            GPIO18: Some(p.GPIO18),
            GPIO19: Some(p.GPIO19),
            GPIO20: Some(p.GPIO20),
            GPIO21: Some(p.GPIO21),
            GPIO26: Some(p.GPIO26),
            GPIO27: Some(p.GPIO27),
            GPIO28: Some(p.GPIO28),
            GPIO29: Some(p.GPIO29),
            GPIO30: Some(p.GPIO30),
            GPIO31: Some(p.GPIO31),
            GPIO32: Some(p.GPIO32),
            GPIO33: Some(p.GPIO33),
            GPIO34: Some(p.GPIO34),
            GPIO35: Some(p.GPIO35),
            GPIO36: Some(p.GPIO36),
            GPIO37: Some(p.GPIO37),
            GPIO38: Some(p.GPIO38),
            GPIO39: Some(p.GPIO39),
            GPIO40: Some(p.GPIO40),
            GPIO41: Some(p.GPIO41),
            GPIO42: Some(p.GPIO42),
            GPIO43: Some(p.GPIO43),
            GPIO44: Some(p.GPIO44),
            GPIO45: Some(p.GPIO45),
            GPIO46: Some(p.GPIO46),
            GPIO47: Some(p.GPIO47),
            GPIO48: Some(p.GPIO48),

            UART0: Some(p.UART0),
            UART1: Some(p.UART1),
            UART2: Some(p.UART2),

            I2C0: Some(p.I2C0),
            I2C1: Some(p.I2C1),

            I2S0: Some(p.I2S0),
            I2S1: Some(p.I2S1),

            SPI0: Some(p.SPI0),
            SPI1: Some(p.SPI1),
            SPI2: Some(p.SPI2),

            LEDC: Some(p.LEDC),

            TWAI0: Some(p.TWAI0),

            PCNT: Some(p.PCNT),

            RMT: Some(p.RMT),

            TIMG0: Some(p.TIMG0),
            TIMG1: Some(p.TIMG1),

            ADC1: Some(p.ADC1),
            ADC2: Some(p.ADC2),

            FROM_CPU_INTR0: Some(p.FROM_CPU_INTR0),
        }
    }
}
