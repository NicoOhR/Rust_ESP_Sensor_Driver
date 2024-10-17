#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    gpio::Io,
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
    let can_config = twai::TwaiConfiguration::new_no_transceiver(
        peripherals.TWAI0,
        can_rx_pin,
        can_tx_pin,
        CAN_BAUDRATE,
        TwaiMode::SelfTest,
    );

    let mut can = can_config.start();

    let device_id = StandardId::new(0x12).unwrap();

    let mut adc1_config = AdcConfig::new();
    let mut adc1_pin = adc1_config.enable_pin(analog_pin, Attenuation::Attenuation0dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);
    //let delay = Delay::new();

    esp_println::logger::init_logger_from_env();

    loop {
        let pin_value: u16 = nb::block!(adc1.read_oneshot(&mut adc1_pin)).unwrap();
        println!("{}", pin_value);
        let sendable_value = pin_value.to_be_bytes();
        let frame = EspTwaiFrame::new(device_id, &sendable_value).unwrap();
        nb::block!(can.transmit(&frame)).unwrap();
        println!("Sent Frame!");
        //TODO: read from RX to make sure sending is all good on the ESP's end
    }
}
