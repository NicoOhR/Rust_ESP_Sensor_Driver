#![no_std]
#![no_main]

use core::cmp::min;
use embedded_can::Frame;
use esp_backtrace as _;
use esp_hal::{
    analog::adc::{Adc, AdcCalBasic, AdcConfig, Attenuation},
    gpio::{Input, Io, Level, Output, Pull},
    pcnt::{channel, Pcnt},
    prelude::*,
    time,
    time::*,
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
    println!("CPU speed: {}", config.cpu_clock.mhz());
    let peripherals = esp_hal::init(config);
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut test_gpio = Output::new(io.pins.gpio9, Level::High);

    type AdcCal = esp_hal::analog::adc::AdcCalBasic<esp_hal::peripherals::ADC1>;
    let analog_pin = io.pins.gpio3;
    let mut adc1_config = AdcConfig::new();
    let mut adc1_pin =
        adc1_config.enable_pin_with_cal::<_, AdcCal>(analog_pin, Attenuation::Attenuation11dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);

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

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut periodic = PeriodicTimer::new(timg0.timer0);
    let _ = periodic.start(10000.micros()); //period of 100hz cycle
    loop {
        let mut can_data: [u8; 8] = [0; 8];
        let pin_value: u16 = nb::block!(adc1.read_oneshot(&mut adc1_pin)).unwrap();
        for _ in 0..5 {
            test_gpio.toggle(); //testing PCNT
        }

        can_data[..2].copy_from_slice(&pin_value.to_be_bytes());
        can_data[2..4].copy_from_slice(&u0.counter.clone().get().to_be_bytes());
        u0.clear();

        let frame = EspTwaiFrame::new_self_reception(device_id, &can_data).unwrap();
        nb::block!(can.transmit(&frame)).unwrap();

        let start = time::now();
        let _ = nb::block!(periodic.wait());
        let end = time::now();
        println!("{:?}", end - start); //comes out to be about 9500 uS of extra compute time
    }
}
