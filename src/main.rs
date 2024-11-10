#![no_std]
#![no_main]
use core::cmp::min;
use esp_backtrace as _;
use esp_hal::dma_buffers;
use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    dma::{Dma, DmaPriority, DmaRxBuf, DmaTxBuf},
    gpio::{Input, Io, Level, Output, Pull},
    i2c::*,
    pcnt::{channel, Pcnt},
    prelude::*,
    spi::{master::Spi, master::SpiDmaBus, SpiMode},
    time,
    timer::*,
    twai::{self, filter::SingleStandardFilter, EspTwaiFrame, StandardId, TwaiMode},
};
use esp_println::println;
use timg::TimerGroup;

const CAN_BAUDRATE: twai::BaudRate = twai::BaudRate::B250K;

#[entry]
fn main() -> ! {
    #[allow(unused)]
    let mut config = esp_hal::Config::default();

    config.cpu_clock = CpuClock::max();
    let peripherals = esp_hal::init(config);
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut i2c = I2c::new(peripherals.I2C0, io.pins.gpio11, io.pins.gpio10, 100.kHz());

    //SPI
    let sclk = io.pins.gpio0;
    let miso = io.pins.gpio6;
    let mosi = io.pins.gpio8;
    let cs = io.pins.gpio5;

    let mut spi = Spi::new(peripherals.SPI2, 100.kHz(), SpiMode::Mode0)
        .with_sck(sclk)
        .with_mosi(mosi)
        .with_miso(miso);

    // test input for the PCNT
    let mut test_gpio = Output::new(io.pins.gpio9, Level::High);
    let mut cs_output = Output::new(cs, Level::High);
    //ADC Configuration
    type AdcCal = esp_hal::analog::adc::AdcCalBasic<esp_hal::peripherals::ADC1>;
    let analog_pin = io.pins.gpio3;
    let mut adc1_config = AdcConfig::new();
    let mut adc1_pin =
        adc1_config.enable_pin_with_cal::<_, AdcCal>(analog_pin, Attenuation::Attenuation11dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);
    //CAN configuration
    let can_tx_pin = io.pins.gpio2;
    let can_rx_pin = can_tx_pin.peripheral_input(); //loopback for testing

    //change to normal mode and construct to new
    let mut can_config = twai::TwaiConfiguration::new(
        peripherals.TWAI0,
        can_rx_pin,
        can_tx_pin,
        CAN_BAUDRATE,
        TwaiMode::SelfTest,
    );

    let can_filter = SingleStandardFilter::new(b"xxxxxxxxxxx", b"x", [b"xxxxxxxx", b"xxxxxxxx"]);

    can_config.set_filter(can_filter);

    let mut can = can_config.start();

    let device_id = StandardId::new(0x12).unwrap(); //make ID into env var

    //PCNT Configuration
    let pcnt = Pcnt::new(peripherals.PCNT);
    let u0 = pcnt.unit0;
    let ch0 = &u0.channel0;
    let wheel_speed_sensor = Input::new(io.pins.gpio4, Pull::Up);

    u0.set_high_limit(Some(255)).unwrap();
    u0.set_filter(Some(min(10u16 * 80, 1023u16))).unwrap();
    u0.clear();
    ch0.set_edge_signal(wheel_speed_sensor.peripheral_input());
    ch0.set_input_mode(channel::EdgeMode::Increment, channel::EdgeMode::Hold);
    u0.listen();
    u0.resume();

    esp_println::logger::init_logger_from_env();

    //Timer Config
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut periodic = PeriodicTimer::new(timg0.timer0);
    let _ = periodic.start(10000.micros()); //period of 100hz cycle

    //Variables for the hyperloop
    let mut can_data: [u8; 8] = [0; 8];
    let mut pin_value: u16;
    let mut start: esp_hal::time::Instant;
    let mut end: esp_hal::time::Instant;
    let mut frame: EspTwaiFrame;
    let mut extern_adc_value: [u8; 2] = [2; 2];
    let mut dlhr_data: [u8; 8] = [0; 8];

    loop {
        //read single shot of data from the DLHR
        let _ = i2c.write_read(41, &[0xAC], &mut dlhr_data);
        //println!("{:?}", &dlhr_data);
        frame = EspTwaiFrame::new_self_reception(device_id, &dlhr_data).unwrap();
        nb::block!(can.transmit(&frame)).unwrap();

        pin_value = nb::block!(adc1.read_oneshot(&mut adc1_pin)).unwrap();
        cs_output.toggle();
        extern_adc_value[0] = spi.read_byte().unwrap();
        extern_adc_value[1] = spi.read_byte().unwrap();
        cs_output.toggle();
        println!("{:?}", extern_adc_value);
        for _ in 0..5 {
            test_gpio.toggle(); //testing PCNT
        }

        can_data[..2].copy_from_slice(&pin_value.to_be_bytes());
        can_data[2..4].copy_from_slice(&u0.counter.clone().get().to_be_bytes());
        can_data[4..6].copy_from_slice(&extern_adc_value);

        frame = EspTwaiFrame::new_self_reception(device_id, &can_data).unwrap();
        nb::block!(can.transmit(&frame)).unwrap();

        u0.clear();

        start = time::now();
        let _ = nb::block!(periodic.wait());
        end = time::now();
        //println!("{}", end - start);
    }
}
