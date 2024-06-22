#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

// driver code dor the dht11 sensor module I'm using to read temp/humidity
use dht_sensor::*;
// an embassy executor is what enables us to have async code
use embassy_executor::Spawner;
// the code used to configure wifi connectivity is scattared across embassy_net, esp_wifi, and embedded_svc
use embassy_net::{dns::DnsQueryType, tcp::TcpSocket, Config, Stack, StackResources};
// embassy_sync is for sharing memory across tasks
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
// nice timer to use instead of blocking hardware based delays
use embassy_time::{Duration, Timer};
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
// no_std rust code needs to define a non_std panic handler, to print what went wrong in case of a panic etc.
use esp_backtrace as _;
// println over serial port from esp board
use esp_println::println;
use esp_wifi::{
    initialize,
    wifi::{WifiController, WifiDevice, WifiStaDevice, WifiState},
    EspWifiInitFor,
};
// hardware abstraction code for esp board
use hal::{
    clock::ClockControl, embassy, peripherals::Peripherals, prelude::*, timer::TimerGroup, Delay,
    Rng, IO,
};
// no_std compatible string. fixed-size and stack allocated
use heapless::String;
// mqtt client code
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig, client_config::MqttVersion},
    utils::rng_generator::CountingRng,
};
// macro to give a variable a static lifetime. would like to minimize use of this as much as possible
use static_cell::make_static;

// mod dht;

// enum to differentiate between tmp/humidity data from the dht sensor, and the flake detector sensor data
#[derive(Debug)]
enum PubPacket {
    Temp(i8, u8),
    Other(bool),
}
// / use embassy_sync::blocking_mutex::raw::NoopRawMutex;
// static SHARED_CHANNEL: Channel<CriticalSectionRawMutex, u32, 4> = Channel::new();
// static STRING_CHANNEL: Channel<CriticalSectionRawMutex, String<8>, 4> = Channel::new();

// a queue (Channel) supported by a mutex which disables interrupts when mutex is locked.
// fills with PubPackets in order in which they were sent
// packets are removed from queue when they are sent. if the queue is full, async awaits until a spot is freed
static SHARED_CHANNEL: Channel<CriticalSectionRawMutex, PubPacket, 4> = Channel::new();

// pull login info for wifi and mqtt broker from environment variables. I set these in my devshell flake,
// which automatically unencrypts them from .age encrypted files
const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");
const MQTT_PASSWORD: &str = env!("MQTT_PASS");
const MQTT_USER: &str = env!("MQTT_USER");

#[main]
async fn main(spawner: Spawner) -> ! {
    println!("SSID set as: {}", SSID);
    println!("MQTT_USER set as: {}", MQTT_USER);
    println!("MQTT_PASSWORD set as: {}", MQTT_PASSWORD);
    // get singleton instances of our boards peripherals. usart, pins, that kind of thing
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();

    // instantiate io handle to configure some pins
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    // I'm using gpio pin 1 as an open drain output for my dht sensor. have to borrow mutable for static lifetime currently
    // let dht_pin = make_static!(io.pins.gpio1.into_open_drain_output());
    let dht_pin = make_static!(io.pins.gpio1.into_open_drain_output());

    // pin 2 is connected to the flame sensor. I pull the pin low, and the sensor pulls it high when it detects flame
    // just reading from this pin so it doesn't need to be mutable or anything
    let flame_pin = io.pins.gpio2.into_inverted_pull_down_input();

    // get handle for the boards internal hardware clocks, with default rate, and freeze them so they cannot change during runtime
    let clocks = ClockControl::max(system.clock_control).freeze();
    let timer = hal::systimer::SystemTimer::new(peripherals.SYSTIMER).alarm0;
    // wifi needs a random seed. the board has hardware that can generate a random number, which we use as a seed later
    let mut hardware_rng = Rng::new(peripherals.RNG);
    let seed = hardware_rng.random();
    // produce a config which the on-board wifi module needs
    let wifi_init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        hardware_rng,
        system.radio_clock_control,
        &clocks,
    )
    .expect("error initializing wifi");
    // the dht driver code I'm using uses a hardware delay. might change if I write my own driver but it works fine for now
    let delay = make_static!(Delay::new(&clocks));

    // get handle on on-board wifi module
    let wifi = peripherals.WIFI;
    // get wifi interface and controller after wifi module is set up with the config we set
    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(&wifi_init, wifi, WifiStaDevice)
            .expect("couldn't create wifi interface");

    // embassy (the async runtime) needs timers and clocks to orchestrate tasks
    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    embassy::init(&clocks, timer_group0);

    // configure dhcp with defaults so I the board can get assigned a dynamic IP
    let dhcp_conf = Config::dhcpv4(Default::default());

    // init wifi stack, give it static lifetime
    let stack: &'static Stack<_> = make_static!(Stack::new(
        wifi_interface,
        dhcp_conf,
        make_static!(StackResources::<3>::new()),
        seed as u64
    ));

    // spawn two tasks. tasks do not Return, but are "awaited", letting other code run
    match spawner.spawn(connection(controller)) {
        Ok(()) => println!("spawning connection task... are we still connected to wifi?"),
        Err(e) => println!("{e:?}"),
    }
    match spawner.spawn(net_task(stack)) {
        Ok(()) => println!("net task ran fine"),
        Err(e) => println!("{e:?}"),
    }

    // wait for link to be up
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // wait to get assigned an IP
    loop {
        if let Some(config) = stack.config_v4() {
            println!("got assigned an IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // lookup ip of broker from dns
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
        // create a tcp socket which will be used to connect to the remote broker. needs buffers for up/down data
        let mut rx_buffer = [0; 4096];
        let mut tx_buffer = [0; 4096];
        let mut tcp_socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

        // socker will disconnect after 10 secs of inactivity
        tcp_socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        println!("connecting...");
        // connecto to broker
        tcp_socket
            .connect(outer_heaven)
            .await
            .map_err(|err| println!("connect error {:?}", err))
            .ok();
        println!("connected!");
        // configure mqtt client with mqtt version and username/password
        let mut mqtt_conf: ClientConfig<'_, 4, CountingRng> =
            ClientConfig::new(MqttVersion::MQTTv5, CountingRng(20000));
        mqtt_conf.add_username(MQTT_USER);
        mqtt_conf.add_password(MQTT_PASSWORD);

        // buffers for reading and writing to broker
        let mut r_buffer = [0; 225];
        let mut w_buffer = [0; 225];

        // initlaiize mqtt client with socket, config, buffers and buffer length
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

        // spawn task that reads sensor data from dht
        match spawner.spawn(send_temp(dht_pin, delay)) {
            Ok(()) => println!("spawned send task 1"),
            Err(e) => println!("{e:?}"),
        }

        // spawn task that attempts to detect a fire
        match spawner.spawn(detect_flame(flame_pin)) {
            Ok(()) => println!("spawned send task 2"),
            Err(e) => println!("{e:?}"),
        }

        // loop which checks if there is a packet in the SHARED_CHANNEL queue, which is populated by the send_temp and detect_flame tasks
        // and publishes messages to the broker according to the type of packet it received
        loop {
            // wait to get a packet in queue
            let msg = SHARED_CHANNEL.receive().await;

            // match the type of packet. either tmp/humididty data or a bool indicating if there is a fire
            match msg {
                PubPacket::Temp(temp, humi) => {
                    // construct a stack allocated string from the tmp, and append with _tmp for clarity
                    let mut send_t: String<20> =
                        String::try_from(temp).expect("failed to create heapless string");
                    send_t
                        .push_str("_tmp")
                        .expect("failed to create heapless string");
                    // construct a stack allocated string from the humidity data, and append with _humidity for clarity
                    let mut send_h: String<30> =
                        String::try_from(humi).expect("failed to create heapless string");
                    send_h
                        .push_str("_humidity")
                        .expect("failed to append string to message");
                    // publish tmp to broker as bytes, with QoS 0
                    mqtt_client
                        .send_message(
                            "esp32/shazbot/test",
                            send_t.as_bytes(),
                            rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0,
                            true,
                        )
                        .await
                        .expect("unable to send message");
                    // publish humidity to broker as bytes, with QoS 0
                    mqtt_client
                        .send_message(
                            "esp32/shazbot/test",
                            send_h.as_bytes(),
                            rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0,
                            true,
                        )
                        .await
                        .expect("unable to send message");
                    println!("just sent a message over mqtt");
                }
                // if the packet is of this type, it is guaranteed to be a bool which equals true. but we check anyway
                // then just send the message FIRE to the broker
                PubPacket::Other(val) => {
                    if val {
                        mqtt_client
                            .send_message(
                                "esp32/shazbot/test",
                                b"FIRE",
                                rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0,
                                true,
                            )
                            .await
                            .expect("unable to send message");
                        println!("just sent a message over mqtt");
                    }
                }
            }
            // await this loop, so our tasks can run
            Timer::after(Duration::from_millis(400)).await;
        }
    }
}

// extract the long OpenDrain type
type OpenDrainPin = hal::gpio::GpioPin<hal::gpio::Output<hal::gpio::OpenDrain>, 1>;
#[embassy_executor::task]
async fn send_temp(pin: &'static mut OpenDrainPin, delay: &'static mut Delay) {
    loop {
        // get a reading from the dht sensor, or print an error over the serial port
        match dht11::Reading::read(delay, pin) {
            Ok(dht11::Reading {
                temperature,
                relative_humidity,
            }) => {
                // construct a packet enum from the data
                let send_reading = PubPacket::Temp(temperature, relative_humidity);
                // try to insert packet in shared queue, await if full until inserted
                SHARED_CHANNEL.send(send_reading).await;
                // await 10 seconds before taking another reading. long wait because I don't expect tmp to change very fast
                Timer::after(Duration::from_secs(10)).await;
            }
            Err(e) => println!("error taking dht reading: {e:?}"),
        }
    }
}

// extract long PullDown pin type
type PullDownInput = hal::gpio::GpioPin<hal::gpio::InvertedInput<hal::gpio::PullDown>, 2>;
#[embassy_executor::task]
async fn detect_flame(pin: PullDownInput) {
    loop {
        // if the flame sensor has pulled the pin high, a fire has been detected, so we send a packet to the shared queue
        if pin.is_input_high() {
            let flame_reading = PubPacket::Other(true);
            SHARED_CHANNEL.send(flame_reading).await;
        }
        Timer::after(Duration::from_millis(200)).await;
    }
}

// async task to establish connection to wifi, and reestablish the connection if we are disocnnected
#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("trying to connect ");
    println!("device capabilities {:?}", controller.get_capabilities());
    // check if we are connected
    loop {
        if esp_wifi::wifi::get_wifi_state() == WifiState::StaConnected {
            controller
                .wait_for_event(esp_wifi::wifi::WifiEvent::StaDisconnected)
                .await;
            Timer::after(Duration::from_millis(5000)).await;
        }
        // check if wifi controller is started, and if it is, create a config for client with the wifi's SSID and password
        // and default for everything else
        if !matches!(controller.is_started(), Ok(true)) {
            let client_conf = Configuration::Client(ClientConfiguration {
                ssid: SSID.try_into().unwrap(),
                password: PASSWORD.try_into().unwrap(),
                ..Default::default()
            });
            // try to start wifi, and wait until it starts
            controller.set_configuration(&client_conf).unwrap();
            println!("starting wifi");
            controller.start().await.unwrap();
            println!("Wifi Started");
        }
        println!("Looks like we're about to connect!");

        // print an error and wait a bit if we failed to connect to wifi, or just print that connection succeeded
        match controller.connect().await {
            Ok(_) => println!("Connected to Wifi!"),
            Err(e) => {
                println!("Failed to connect: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

// need this here for some reason relating to how the esp-hal code works. think it consumed network events? mysterious
#[embassy_executor::task]
async fn net_task(stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>) {
    stack.run().await
}
