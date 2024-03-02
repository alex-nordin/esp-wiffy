#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use embassy_executor::Spawner;
use embassy_net::{dns::DnsQueryType, tcp::TcpSocket, Config, Stack, StackResources};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use esp_backtrace as _;
use esp_println::println;
use esp_wifi::{
    initialize,
    wifi::{WifiController, WifiDevice, WifiStaDevice, WifiState},
    EspWifiInitFor,
};
use hal::{
    clock::ClockControl, embassy, peripherals::Peripherals, prelude::*, timer::TimerGroup, Rng, IO,
};
use heapless::String;
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig, client_config::MqttVersion},
    utils::rng_generator::CountingRng,
};
use static_cell::make_static;

#[derive(Debug)]
enum PubPacket {
    Temp(i32),
    Other(i32),
}
// / use embassy_sync::blocking_mutex::raw::NoopRawMutex;
// static SHARED_CHANNEL: Channel<CriticalSectionRawMutex, u32, 4> = Channel::new();
// static STRING_CHANNEL: Channel<CriticalSectionRawMutex, String<8>, 4> = Channel::new();
static SHARED_CHANNEL: Channel<CriticalSectionRawMutex, PubPacket, 4> = Channel::new();

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");
const MQTT_PASSWORD: &str = env!("MQTT_PASS");
const MQTT_USER: &str = env!("MQTT_USER");

#[main]
async fn main(spawner: Spawner) -> ! {
    println!("SSID set as: {}", SSID);
    println!("MQTT_USER set as: {}", MQTT_USER);
    println!("MQTT_PASSWORD set as: {}", MQTT_PASSWORD);
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let clocks = ClockControl::max(system.clock_control).freeze();
    let timer = hal::systimer::SystemTimer::new(peripherals.SYSTIMER).alarm0;
    let mut hardware_rng = Rng::new(peripherals.RNG);
    let seed = hardware_rng.random();
    let wifi_init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        hardware_rng,
        system.radio_clock_control,
        &clocks,
    )
    .expect("error initializing wifi");

    let wifi = peripherals.WIFI;
    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(&wifi_init, wifi, WifiStaDevice)
            .expect("couldn't create wifi interface");

    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    embassy::init(&clocks, timer_group0);

    let analog_pin = io.pins.gpio0.into_analog();
    let analog = peripherals.APB_SARADC.split();

    let dhcp_conf = Config::dhcpv4(Default::default());

    let stack: &'static Stack<_> = make_static!(Stack::new(
        wifi_interface,
        dhcp_conf,
        make_static!(StackResources::<3>::new()),
        seed as u64
    ));

    match spawner.spawn(connection(controller)) {
        Ok(()) => println!("spawning connection task... are we still connected to wifi?"),
        Err(e) => println!("{e:?}"),
    }
    match spawner.spawn(net_task(stack)) {
        Ok(()) => println!("net task ran fine"),
        Err(e) => println!("{e:?}"),
    }

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    loop {
        if let Some(config) = stack.config_v4() {
            println!("got assigned an IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    loop {
        let broker_address = match stack
            .dns_query("broker.emqx.io", DnsQueryType::A)
            .await
            .map(|address_vec| address_vec[0])
        {
            Ok(broker_address) => broker_address,
            Err(e) => {
                println!("DNS error: {:?}", e);
                continue;
            }
        };
        println!("broker address: {:?}", broker_address);
        let outer_heaven = (broker_address, 1883);
        let mut rx_buffer = [0; 4096];
        let mut tx_buffer = [0; 4096];
        let mut tcp_socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

        tcp_socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        println!("connecting...");
        tcp_socket
            .connect(outer_heaven)
            .await
            .map_err(|err| println!("connect error {:?}", err))
            .ok();
        println!("connected!");
        // MQTT CONFIG START
        let mut mqtt_conf: ClientConfig<'_, 4, CountingRng> =
            ClientConfig::new(MqttVersion::MQTTv5, CountingRng(20000));
        mqtt_conf.add_username(MQTT_USER);
        mqtt_conf.add_password(MQTT_PASSWORD);

        let mut r_buffer = [0; 225];
        let mut w_buffer = [0; 225];

        let mut mqtt_client = MqttClient::new(
            tcp_socket,
            &mut w_buffer,
            225,
            &mut r_buffer,
            225,
            mqtt_conf,
        );

        match mqtt_client.connect_to_broker().await {
            Ok(()) => (),
            Err(e) => println!("encountered mqtt error: {:?}", e),
        }

        // match spawner.spawn(send_tmp(analog_pin, analog)) {
        //     Ok(()) => println!("spawned send task 1"),
        //     Err(e) => println!("{e:?}"),
        // }
        match spawner.spawn(send()) {
            Ok(()) => println!("spawned send task 1"),
            Err(e) => println!("{e:?}"),
        }

        match spawner.spawn(send_2()) {
            Ok(()) => println!("spawned send task 2"),
            Err(e) => println!("{e:?}"),
        }

        loop {
            let msg = SHARED_CHANNEL.receive().await;

            match msg {
                PubPacket::Temp(_val) => {
                    let val_int = _val as i32;
                    let mut send: String<20> =
                        String::try_from(val_int).expect("failed to create heapless string");
                    send.push_str("_tmp").unwrap();
                    mqtt_client
                        .send_message(
                            "esp32/shazbot/test",
                            send.as_bytes(),
                            rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0,
                            true,
                        )
                        .await
                        .expect("unable to send message");
                    println!("just sent a message over mqtt");
                }
                PubPacket::Other(_val) => {
                    let mut send: String<20> =
                        String::try_from(_val).expect("failed to create heapless string");
                    send.push_str("_other").unwrap();
                    mqtt_client
                        .send_message(
                            "esp32/shazbot/test",
                            send.as_bytes(),
                            rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0,
                            true,
                        )
                        .await
                        .expect("unable to send message");
                    println!("just sent a message over mqtt");
                }
            }
            Timer::after(Duration::from_millis(400)).await;
        }
    }
}

#[embassy_executor::task]
async fn send() {
    loop {
        let reading = PubPacket::Temp(99);
        SHARED_CHANNEL.send(reading).await;
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn send_2() {
    loop {
        let other_reading = PubPacket::Other(444);
        SHARED_CHANNEL.send(other_reading).await;
        Timer::after(Duration::from_secs(2)).await;
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("trying to connect ");
    println!("device capabilities {:?}", controller.get_capabilities());
    loop {
        if esp_wifi::wifi::get_wifi_state() == WifiState::StaConnected {
            controller
                .wait_for_event(esp_wifi::wifi::WifiEvent::StaDisconnected)
                .await;
            Timer::after(Duration::from_millis(5000)).await;
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_conf = Configuration::Client(ClientConfiguration {
                ssid: SSID.try_into().unwrap(),
                password: PASSWORD.try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_conf).unwrap();
            println!("starting wifi");
            controller.start().await.unwrap();
            println!("Wifi Started");
        }
        println!("Looks like we're about to connect!");

        match controller.connect().await {
            Ok(_) => println!("Connected to Wifi!"),
            Err(e) => {
                println!("Failed to connect: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>) {
    stack.run().await
}
