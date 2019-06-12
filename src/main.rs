//extern crate bytes;
//extern crate futures;
//extern crate tokio;
//extern crate tokio_io;
//extern crate tokio_serial;

use std::{env, io, str};
use std::time::{Duration, Instant};
use tokio::prelude::*;

use tokio::io::{write_all, read_exact, read_to_end};

use bytes::BytesMut;

use futures::{Future, Stream};
use tokio::runtime::current_thread::Runtime;
use tokio::codec::{Encoder, Decoder};
use futures::future::ok;

const DEFAULT_TTY: &str = "/dev/serial/by-id/usb-Fake_company_Serial_port_TEST-if00";

struct FramedCodec(usize);

impl Decoder for FramedCodec {
    type Item = (usize, Vec<u8>);
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            Ok(None)
        } else {
            let offset = self.0;
            let mut vec = Vec::new();
            let data = src.take();
            vec.extend_from_slice(&data);
            self.0 += vec.len();
            Ok(Some((offset, vec)))
        }
    }
}

impl Encoder for FramedCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(&item);
        Ok(())
    }
}

fn gen_random_str(size: usize) -> String {
    use rand::Rng;

    let chars = "abcdefghijklmnopqrstuvwxyz0123456789";

    let mut rng = rand::thread_rng();

    (0..size).map(|_| {
        let r: usize = rng.gen_range(0, chars.len());
        chars.chars().nth(r).unwrap()
    }).collect()
}

fn find_offset(haystack: &[u8], needle: &[u8], hint_offset: usize) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn main() {
    let mut args = env::args();
    let tty_path = args.nth(1).unwrap_or_else(|| DEFAULT_TTY.into());

    let settings = tokio_serial::SerialPortSettings::default();
    let mut port = tokio_serial::Serial::from_path(tty_path, &settings).unwrap();
    //let mut port = tokio_serial::Serial::pair().unwrap();
    #[cfg(unix)]
    port.set_exclusive(false).expect("Unable to set serial port exlusive");

    const DATA_SIZE: usize = 2000;

    let tx_str = gen_random_str(DATA_SIZE);
    let rx_str = tx_str.to_ascii_uppercase();

    let tx_bytes = tx_str.into_bytes();
    let rx_bytes = rx_str.into_bytes();
    assert_eq!(tx_bytes.len(), DATA_SIZE);
    assert_eq!(rx_bytes.len(), DATA_SIZE);

    println!("Data prepaired");

    let (tx, rx) = FramedCodec(0).framed(port).split();

    let writer = tx.send(tx_bytes).into_future().map(|_| {
        println!("writer finished");
        ()
    }).map_err(|e| panic!(e));

    let reader = rx.skip_while(move |(offset, data)| {
        let offset = *offset;
        println!("packet offset {} len {}", offset, data.len());
        let rx_bytes2 = &rx_bytes[offset..];
        for i in 0..data.len() {
            if data[i] != rx_bytes2[i] {
                println!("  wrong data at {}: 0x{:02x} != 0x{:02x}", offset+i, data[i], rx_bytes2[i]);
                if let Some(offset2) = find_offset(&rx_bytes, &data, offset) {
                    println!("  correct offset: {}, packet offset: {} ({})", offset2, offset, offset2 - offset);
                }
                break
            }
        }
        let total_size = offset + data.len();
        ok(total_size < DATA_SIZE)
    }).into_future().map(|_| {
        println!("reader finished");
        ()
    }).map_err(|e| panic!(e));


    let mut rt = Runtime::new().unwrap();

    let start = Instant::now();
    rt.spawn(reader);
    rt.spawn(writer);
    rt.run().unwrap();
    let duration = start.elapsed();

    let elapsed = duration.as_secs() as f64 + (duration.subsec_micros() as f64) * 0.000_001;
    let throughput = (DATA_SIZE * 8) as f64 / 1_000_000.0 / elapsed;

    println!("Time elapsed: {:?}, throughput is {:.3} Mbit/s", duration, throughput);
}
