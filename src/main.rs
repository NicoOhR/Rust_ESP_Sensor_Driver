#![no_std]
#![no_main]

use core::cmp::min;
use embedded_can::Frame;
use esp_backtrace as _;
use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    gpio::{Input, Io, Pull},
    pcnt::{channel, Pcnt},
    prelude::*,
    twai::{self, filter::SingleStandardFilter, EspTwaiFrame, StandardId, TwaiMode},
};
use esp_println::println;

const CAN_BAUDRATE: twai::BaudRate = twai::BaudRate::B250K;

#[entry]
fn main() -> ! {
    #[allow(unused)]
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);
    let analog_pin = io.pins.gpio3;

    let can_tx_pin = io.pins.gpio2;
    let can_rx_pin = can_tx_pin.peripheral_input(); //loopback for testing

    //change to normal mode and construct to new
    let mut can_config = twai::TwaiConfiguration::new_no_transceiver(
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
    u0.set_high_limit(Some(255)).unwrap();
    u0.set_low_limit(Some(0)).unwrap();
    u0.set_filter(Some(min(10u16 * 80, 1023u16))).unwrap();
    u0.clear();
    let ch0 = &u0.channel0;
    let wheel_speed_sensor = Input::new(io.pins.gpio4, Pull::Up);
    ch0.set_edge_signal(wheel_speed_sensor.peripheral_input());
    ch0.set_input_mode(channel::EdgeMode::Increment, channel::EdgeMode::Hold);
    u0.listen();
    u0.resume();

    let mut adc1_config = AdcConfig::new();
    let mut adc1_pin = adc1_config.enable_pin(analog_pin, Attenuation::Attenuation0dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);

    //let delay = Delay::new();

    esp_println::logger::init_logger_from_env();

    loop {
        let pin_value: u16 = nb::block!(adc1.read_oneshot(&mut adc1_pin)).unwrap();
        println!("{}", pin_value); //read ADC

        let sendable_value = pin_value.to_be_bytes(); //convert to bytes
        let frame = EspTwaiFrame::new(device_id, &sendable_value).unwrap();
        nb::block!(can.transmit(&frame)).unwrap(); //transmit
        println!("Sent Frame!");

        let response = nb::block!(can.receive()).unwrap();
        let response_data = response.data();
        println!("Recieved Frame : {response_data:?}");

        let counter = u0.counter.clone();
        println!("Pulses this cycle: {}", counter.get());
    }
}
