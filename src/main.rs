#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use embassy_executor::Spawner;
use embassy_net::{tcp::TcpSocket, Config, Ipv4Address, Stack, StackResources};
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
    clock::ClockControl, embassy, peripherals::Peripherals, prelude::*, timer::TimerGroup, Rng,
};
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig, client_config::MqttVersion},
    utils::rng_generator::CountingRng,
};
use static_cell::make_static;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");
const MQTT_PASSWORD: &str = env!("MQTT_PASS");
const MQTT_USER: &str = "peep";

#[main]
async fn main(spawner: Spawner) -> ! {
    println!("SSID set as: {}", SSID);
    println!("MQTT_USER set as: {}", MQTT_USER);
    println!("MQTT_PASSWORD set as: {}", MQTT_PASSWORD);
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();

    let clocks = ClockControl::max(system.clock_control).freeze();
    let timer = hal::systimer::SystemTimer::new(peripherals.SYSTIMER).alarm0;
    let wifi_init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        Rng::new(peripherals.RNG),
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();

    let wifi = peripherals.WIFI;
    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(&wifi_init, wifi, WifiStaDevice).unwrap();

    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    embassy::init(&clocks, timer_group0);

    let dhcp_conf = Config::dhcpv4(Default::default());

    let seed = 9128;

    let stack = &*make_static!(Stack::new(
        wifi_interface,
        dhcp_conf,
        make_static!(StackResources::<3>::new()),
        seed
    ));
    // let stack: &'static Stack<_> = make_static!(Stack::new(
    //     wifi_interface,
    //     dhcp_conf,
    //     make_static!(StackResources::<3>::new()),
    //     seed
    // ));

    match spawner.spawn(connection(controller)) {
        Ok(()) => println!("spawning connection task... are we still connected to wifi?"),
        Err(e) => println!("{e:?}"),
    }
    // spawner.spawn(connection(controller)).ok();
    // spawner.spawn(net_task(&stack)).ok();
    match spawner.spawn(net_task(&stack)) {
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
            println!("Got assigned an IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    loop {
        let mut rx_buffer = [0; 4096];
        let mut tx_buffer = [0; 4096];
        let localhost = (Ipv4Address::new(127, 0, 0, 1), 1883);
        let mut tcp_socket = TcpSocket::new(&stack, &mut rx_buffer, &mut tx_buffer);

        tcp_socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        println!("connecting...");
        // let tcp_response = tcp_socket.connect(localhost).await;
        if let Err(e) = tcp_response {
            println!("connect error: {:?}", e);
            continue;
        }
        println!("connected!");
        // // println!("connected!");
        // // match tcp_socket.connect(localhost).await {
        // //     Ok(()) => println!("connected to localhost"),
        // //     Err(e) => println!("connection error: {:?}", e),
        // // }
        // loop {
        //     match tcp_socket.connect(localhost).await {
        //         Ok(()) => {
        //             println!("connected to localhost");
        //             break;
        //         }
        //         Err(e) => println!("connection error: {:?}", e),
        //     }
        //     Timer::after(Duration::from_millis(500)).await;
        // }
        // tcp_response = tcp_socket.connect(localhost).await;
        // let rng_counter = CountingRng(50000);
        let mut mqtt_conf: ClientConfig<'_, 5, CountingRng> =
            ClientConfig::new(MqttVersion::MQTTv5, CountingRng(50000));
        mqtt_conf.add_client_id("esp");
        mqtt_conf.add_username(MQTT_USER);
        mqtt_conf.add_password(MQTT_PASSWORD);

        let mut r_buffer = [0; 225];
        let mut w_buffer = [0; 225];

        let mut mqtt_client = MqttClient::new(
            tcp_socket,
            &mut w_buffer,
            100,
            &mut r_buffer,
            100,
            mqtt_conf,
        );

        match mqtt_client.connect_to_broker().await {
            Ok(()) => (),
            Err(e) => println!("encountered mqtt error: {:?}", e),
        }

        loop {
            mqtt_client
                .send_message(
                    "test",
                    b"hey, I'm an esp32c3",
                    rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS2,
                    true,
                )
                .await
                .unwrap();
            Timer::after(Duration::from_secs(5)).await;
        }
    }
}

// #[embassy_executor::task]
// async fn mqtt_connect() {
//     let mut msg_buffer = [0u8; 4096];

//     let mut rx_buffer = [0; 4096];
//     let mut tx_buffer = [0; 4096];
//     let localhost = (Ipv4Address::new(127, 0, 0, 1), 1883);
//     let mut tcp_socket = TcpSocket::new(&stack, &mut rx_buffer, &mut tx_buffer);

//     // let tcp_response = tcp_socket.connect(localhost).await;
//     match tcp_socket.connect(localhost).await {
//         Ok(()) => (),
//         Err(e) => println!("connection error: {:?}", e),
//     }
//     // tcp_response = tcp_socket.connect(localhost).await;
//     let rng_counter = CountingRng(50000);
//     let mut mqtt_conf = ClientConfig::new(MqttVersion::MQTTv5, rng_counter);
//     mqtt_conf.add_username(MQTT_USER);
//     mqtt_conf.add_password(MQTT_PASSWORD);

//     let mut r_buffer = [0; 225];
//     let mut w_buffer = [0; 225];

//     // network_driver: T,
//     // buffer: &'a mut [u8],
//     // buffer_len: usize,
//     // recv_buffer: &'a mut [u8],
//     // recv_buffer_len: usize,
//     // config: ClientConfig<'a, MAX_PROPERTIES, R>,
//     let mut mqtt_client = MqttClient::new(
//         tcp_socket,
//         &mut w_buffer,
//         100,
//         &mut r_buffer,
//         100,
//         mqtt_conf,
//     );

//     match mqtt_client.connect_to_broker().await {
//         Ok(()) => (),
//         Err(e) => println!("encountered mqtt error: {:?}", e),
//     }

//     loop {
//         mqtt_client.send_message(
//             "test",
//             b"hey, I'm an esp32c3",
//             rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS2,
//             true,
//         );
//         Timer::after(Duration::from_secs(5)).await;
//     }
// }

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("trying to connect ");
    println!("device capabilities {:?}", controller.get_capabilities());
    loop {
        match esp_wifi::wifi::get_wifi_state() {
            WifiState::StaConnected => {
                controller
                    .wait_for_event(esp_wifi::wifi::WifiEvent::StaDisconnected)
                    .await;
                Timer::after(Duration::from_millis(5000)).await;
            }
            _ => {}
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
