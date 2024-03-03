// use embassy_sync::pubsub::Error;
// use embassy_time::{Duration, Instant, Timer};
// use esp_println::println;
// use hal::{embassy, peripherals::Peripherals, prelude::*};
// type Pin = hal::gpio::GpioPin<hal::gpio::Output<hal::gpio::OpenDrain>, 1>;
// const TIMEOUT_US: u16 = 1_000;

// #[derive(Copy, Clone, Default, Debug)]
// pub struct Measurement {
//     pub temperature: i16,
//     pub humidity: u16,
// }

// pub struct Dht {
//     pin: Pin,
// }

// impl Dht {
//     pub fn new(pin: Pin) -> Self {
//         Dht { pin }
//     }
//     async fn handshake(&mut self) -> Result<(), DhtError> {
//         self.pin.into_floating_input();
//         Timer::after(Duration::from_millis(1)).await;

//         self.pin.set_low();
//         Timer::after(Duration::from_millis(20)).await;

//         self.pin.into_floating_input();
//         Timer::after(Duration::from_micros(40)).await;

//         self.read_bit();

//         Ok(())
//     }

//     async fn read_bit(&mut self) -> Result<bool, DhtError> {
//         let low = self.count_pulse(true).await;
//         let high = self.count_pulse(false).await;
//         // high.unwrap();
//         // low.unwrap();
//         // match high {
//         //     Ok(n) => {
//         //         let num = n;
//         //     }
//         //     Err(e) => println!("error reading value high"),
//         // }
//         // match low {
//         //     Ok(m) => {
//         //         let num2 = m;
//         //     }
//         //     Err(e) => println!("error reading value low"),
//         // }

//         Ok(high > low)
//     }

//     async fn count_pulse(&mut self, level: bool) -> Result<u64, DhtError> {
//         let start = Instant::now().as_micros();

//         let mut count = 0;

//         while self.read_line() != level {
//             count += 1;
//             if count > TIMEOUT_US {
//                 Err;
//             }
//             Timer::after(Duration::from_micros(1)).await;
//         }

//         Ok(Instant::now().as_micros().wrapping_sub(start))
//     }
// }

// #[derive(Debug)]
// pub enum DhtError {
//     /// Timeout during communication.
//     Timeout,
//     /// checksum mismatch.
//     CrcMismatch,
//     /// GPIO error.
//     Gpio,
// }

// // impl core::fmt::Display for MyErrors {
// //     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
// //         match self {
// //             Self::LetsFixThisTomorrowError => write!(f, "Doesn't look too bad"),
// //             Self::ThisDoesntLookGoodError => write!(f, "Let's get to work"),
// //             Self::ImmaGetFiredError => write!(f, "Wish me luck"),
// //         }
// //     }
// // }

// // async fn handshake(pin: Pin) -> Result<(), DhtError> {
// //     pin.into_floating_input();
// //     Timer::after(Duration::from_millis(1)).await;

// //     pin.set_low();
// //     Timer::after(Duration::from_millis(20)).await;

// //     pin.into_floating_input();
// //     Timer::after(Duration::from_micros(40)).await;

// //     read_bit(pin);

// //     Ok(())
// // }

// //return how many us. need to be low for 80 and high for 80

// // fn read_bit(pin: Pin) -> Result<bool, DhtError> {
// //     let low = pin.count_pulse(true);
// //     let high = pin.count_pulse(false);
// //     Ok(high > low)
// // }

// // async fn count_pulse(pin: Pin, level: bool) -> Result<u64, DhtError> {
// //     let start = Instant::now().as_micros();

// //     let mut count = 0;

// //     while pin.read_line() != level {
// //         count += 1;
// //         if count > TIMEOUT_US {
// //             Err;
// //         }
// //         Timer::after(Duration::from_micros(1)).await;
// //     }

// //     Ok(Instant::now().as_micros().wrapping_sub(start))
// // }

// fn read_line(pin: Pin) -> Result<bool, DhtError> {
//     pin.is_high().unwrap();
//     Ok(true)
// }

// pub fn perform_measurement(pin: Pin) -> Result<Measurement, DhtError> {
//     let mut data = [0u8; 5];

//     pin.handshake().unwrap();

//     for i in 0..40 {
//         data[i / 8] <<= 1;
//         if pin.read_bit() {
//             data[i / 8] |= 1;
//         }
//     }

//     pin.count_pulse(true);
//     let check_data = &data[0..3];
//     let checksum = check_data.fold(0, |acc, x| acc + x);

//     if checksum != data[4] {
//         Err
//     }

//     let mut temp = i16::from(data[2] & 0x7f) * 10 + i16::from(data[3]);
//     if data[2] & 0x80 = !0 {
//         temp = -temp;
//     }

//     Ok(Measurement {
//         temperature: temp,
//         humidity: u16::from(data[0]) * 110 + u16::from(data[1]),
//     })
// }
