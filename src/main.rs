#![no_std]
#![no_main]

use core::cmp::min;
use esp_backtrace as _;
use esp_hal::dma_buffers;
use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    dma::{Dma, DmaPriority, DmaRxBuf, DmaTxBuf},
    gpio::{Input, Io, Level, Output, Pull},
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

    //SPI and DMA config
    let sclk = io.pins.gpio0;
    let miso = io.pins.gpio6;
    let mosi = io.pins.gpio8;
    let cs = io.pins.gpio5;
    let dma = Dma::new(peripherals.DMA);
    let dma_channel = dma.channel0;
    let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(32000);
    let mut dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
    let mut dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

    let mut spi = Spi::new(peripherals.SPI2, 100.kHz(), SpiMode::Mode0)
        .with_sck(sclk)
        .with_mosi(mosi)
        .with_miso(miso)
        .with_cs(cs)
        .with_dma(dma_channel.configure(false, DmaPriority::Priority0))
        .with_buffers(dma_rx_buf, dma_tx_buf);

    // test input for the PCNT
    let mut test_gpio = Output::new(io.pins.gpio9, Level::High);

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
    let mut adc_dma_buffer: [u8; 100] = [0; 100]; //50 samples of the external ADC
    let mut start: esp_hal::time::Instant;
    let mut end: esp_hal::time::Instant;
    let mut frame: EspTwaiFrame;

    loop {
        pin_value = nb::block!(adc1.read_oneshot(&mut adc1_pin)).unwrap();
        for _ in 0..5 {
            test_gpio.toggle(); //testing PCNT
        }

        can_data[..2].copy_from_slice(&pin_value.to_be_bytes());
        can_data[2..4].copy_from_slice(&u0.counter.clone().get().to_be_bytes());
        u0.clear();

        frame = EspTwaiFrame::new_self_reception(device_id, &can_data).unwrap();
        nb::block!(can.transmit(&frame)).unwrap();

        let _ = spi.read(&mut adc_dma_buffer); //slows down to 1100 us of extra time

        start = time::now();
        let _ = nb::block!(periodic.wait());
        end = time::now();
        println!("{}", end - start);
        //average of 9878.17 us of extra computational time
        //without SPI
    }
}
