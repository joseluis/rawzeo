// rawzeo::main
//
//! Read raw data from Zeo headband.
//

use std::{
    collections::VecDeque,
    io::{self, Read},
    time::Duration,
};

use circular_buffer::CircularBuffer;
use serialport::{Parity, StopBits};

use rawzeo::DataType;

// TODO:w
// thread_local! {
//     /// The collected subseconds seen during this run.
//     ///
//     /// the value is the number of times seen.
//     static SUBSEC_SEEN: RefCell<HashMap<u16, usize>> = RefCell::new(HashMap::new());
//
//     /// The known list of subseconds.
//     ///
//     /// The `SUBSEC_SEEN` that not appearing in this list should be added.
//     static SUBSEC_KNOWN: [u16; 8] = [2, 4, 6, 8, 10, 12, 14, 16]; // 22 ?
// }

fn main() {
    let port_name = "/dev/ttyUSB0";
    let baud_rate: u32 = 38400;

    let port = serialport::new(port_name, baud_rate)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .timeout(Duration::from_millis(10))
        .open();

    match port {
        Ok(mut port) => {
            let mut buffer = [0; 512];
            let mut ring = CircularBuffer::<512, u8>::new();

            let mut prev_seqnum = None;

            // TODO IMPROVE: receive this data (callback? global?)
            // let mut zeo_timestamp = 0_u32;
            // let mut zeo_version = 0_u32;

            let mut bytes_received;
            let mut result = Ok(None);

            println!("Receiving data on {} at {} baud:", &port_name, &baud_rate);
            loop {
                bytes_received = 0;
                match port.read(&mut buffer) {
                    Ok(n) => {
                        bytes_received = n;
                        println!("EXTENDING ring with {n} bytes");
                        ring.extend_from_slice(&buffer[..n]);
                        result = parse::<512>(&mut prev_seqnum, &mut ring);
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                    Err(e) => eprintln!("{:?}", e),
                };

                if bytes_received > 0 {
                    // TODO: print information
                    if let Ok(Some(ref data)) = result {
                        println!("PARSED: {data:?}");
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open \"{}\". Error: {}", port_name, e);
            ::std::process::exit(1);
        }
    }
}

/// Pretty prints a byte iterator.
#[rustfmt::skip]
fn print_bytes<E: ExactSizeIterator>(bytes: E) where <E as Iterator>::Item: core::fmt::UpperHex {
    print!("[{} B]: ", bytes.len());
    for b in bytes { print!("{b:02X} "); }
    println!();
}

/**
Parses the data coming from the serial port connected to Zeo.

The serial port is set at baud 38400, no parity, one stop bit.
Data is sent Least Significant Byte first.

The serial protocol is:
    `AncllLLTttsidddd`

    * A  is a character starting the message
    * n  is the protocol "version", ie "4"
    * c  is a one byte checksum formed by summing the identifier byte and all
      the data bytes
    * ll is a two byte message length sent LSB first. This length includes the
      size of the data block plus the identifier.
    * LL is the inverse of ll sent for redundancy. If ll does not match !LL, we
      can start looking for the start of the next block immediately, instead of
      reading some arbitrary number of bytes, based on a bad length.
    * T  is the lower 8 bits of Zeo's unix time.
    * tt is the 16-bit sub-second (runs through 0xFFFF in 1second), LSB first.
      NOTE: max value seen is 16, so it's 0xF in 1 second
    * s  is an 8-bit sequence number.
    * i  is the datatype
    * d  is the array of binary data (seems to be 4 len minimum)

The incoming data is cleaned up into packets containing a timestamp,
the raw data output version, and the associated data.

External code can be sent new data as it arrives by adding
themselves to the callback list using the addCallBack function.
It is suggested, however, that external code use the ZeoParser to
organize the data into events and slices of data.
*/
// TODO: IMPROVE return data
fn parse<const LEN: usize>(
    prev_seqnum: &mut Option<u8>,
    ring: &mut CircularBuffer<LEN, u8>,
) -> Result<Option<(u32, u16, u32, DataType, VecDeque<u8>)>, &'static str> {
    // types to return
    let mut tt_ss = 0_u16;
    let mut datatype = DataType::Invalid(255);
    let mut datavec = VecDeque::new();

    let mut zeo_version = 0_u32;
    let mut zeo_time: u32;
    // FIX IMPROVE: this doesn't work
    let mut zeo_time_full = 0_u32;

    // counter of while loop iterations
    let mut while_counter = 0;

    // Check if data length is at least 16 bytes (minimum length of a valid packet)
    while ring.len() > 15 {
        print!("\n» PARSE_{} ", while_counter);
        print_bytes(ring.iter());

        // 1. Parse message start (+2 = 2 bytes)
        let mut start = [0; 2];

        'inner: loop {
            start.swap(0, 1);
            // FIX: fails with the
            start[1] = ring.pop_front().unwrap();
            if &start == b"A4" {
                break 'inner;
            }
        }

        // make sure there's enough bytes left.
        //
        // Otherwise refill the message start and return None.
        // The ring will be filled with more bytes and we'll try again.
        if ring.len() < 14 {
            // or 13?
            println!("> (not enough bytes left: {} )", ring.len());
            ring.push_front(0x34); // 4
            ring.push_front(0x41); // A
            return Ok(None);
        }

        // 2. Parse the checksum byte (+1 = 3 bytes)
        let cksum = ring.pop_front().unwrap();
        println!("> checksum: 0x{cksum:02X} ({cksum})");

        // 3. Parse message length bytes (+4 = 7 bytes)
        let dl = u16::from_le_bytes([ring.pop_front().unwrap(), ring.pop_front().unwrap()]);
        let inv_dl = u16::from_le_bytes([ring.pop_front().unwrap(), ring.pop_front().unwrap()]);
        println!("> dl:{dl} inv:{inv_dl}→(inv:{})", !inv_dl);

        // Check if message lengths match
        if dl != !inv_dl {
            return Err("Invalid message length.");
        }

        // 4. Parse timestamp bytes (+3 = 10 bytes)
        //
        // timestamp low byte
        let tt_lb = ring.pop_front().unwrap();
        // timestamp sub-seconds
        tt_ss = u16::from_le_bytes([ring.pop_front().unwrap(), ring.pop_front().unwrap()]);
        // timestamp floating point subsec
        let tt_fss = (tt_ss.saturating_sub(1)) as f32 / 15.0;
        println!("> tt_lb: 0x{tt_lb:02X} ({tt_lb}), tt_ss:({tt_ss})({tt_fss:.02})");

        // SUBSEC_SEEN.with(|rcell| {
        //     // rcell.borrow_mut().insert(tt_ss, 1); // CHECK
        //     rcell.borrow_mut().entry(tt_ss)
        //         .and_modify(|count| *count += 1)
        //         // .and_modify(|count| count.sum_assign(1)) // CHECK
        //         .or_insert(0);
        // });

        // 5. Parse sequence number byte (+ 1 = 11 bytes)
        let seqnum = ring.pop_front().unwrap();
        println!("> seqnum: {seqnum}");
        // we shouldn't be losing any sequences (after 255 comes 0)
        // but we do, seemingly without fault of our own…...
        if let Some(pseq) = prev_seqnum {
            // debug_assert![pseq.wrapping_add(1) == seqnum];
            // DEBUG
            let prev_seq1 = pseq.wrapping_add(1);
            if prev_seq1 != seqnum {
                println!["we've lost {} sequence(s)!", seqnum - prev_seq1];
            }
        }
        *prev_seqnum = Some(seqnum);

        // 6. Parse data type byte (+1 = 12 bytes)
        let dtype = ring.pop_front().unwrap();
        datatype = DataType::from(dtype);
        println!("> datatype: {datatype}");

        // CHECK whether sometimes there are not enough received bytes to parse the data

        // 7. Parse data bytes
        let datalen = dl - 1;
        // println!("> datalen: {datalen}");

        // TEMP
        if ring.len() < 4 {
            println!(
                "> less than 4 data bytes!: {} {}",
                ring.len(),
                "=".repeat(20)
            );
        }

        // IMPROVE: use ladata::Deque
        datavec = VecDeque::<u8>::with_capacity(datalen as usize);
        for _ in 0..datalen {
            // NOTE sometimes not enough data is received…. E.g.:
            //
            // » PARSE_0 [17 B]: 00 00 41 34 EA 05 00 FA FF 1F 06 00 84 8A 1F 2B B3
            // > checksum: 0xEA (234)
            // > dl:5 inv:65530→(inv:5)
            // > tt_lb: 0x1F (31), tt_ss:(6)(0.00009155413)
            // > seqnum: 132
            // > datatype: ZeoTimestamp
            if let Some(byte) = ring.pop_front() {
                datavec.push_back(byte);
            } else {
                println!(">> warning, not enough data!!");
            }
        }
        print!("> DATA: ");
        print_bytes(datavec.iter());

        // 8. Verify checksum
        if (dtype as u32 + datavec.iter().map(|b| *b as u32).sum::<u32>()) % 256 != cksum as u32 {
            return Err("Invalid checksum.");
        }

        if let DataType::Invalid(_b) = datatype {
            return Err("Bad datatype: {{b:02X}}"); // IMPROVE: use format!
        }

        if datatype == DataType::ZeoTimestamp {
            zeo_time = u32::from_le_bytes([
                datavec.pop_front().unwrap(),
                datavec.pop_front().unwrap(),
                datavec.pop_front().unwrap(),
                datavec.pop_front().unwrap_or(0), // can fail :S
            ]);
            println!("> zeo_time: {}", zeo_time);

            // Construct the full timestamp from the most recently received RTC
            // value in seconds, and the lower 8 bits of the RTC value as of
            // when this object was sent.
            if zeo_time & 0xFF == tt_lb as u32 {
                zeo_time_full = zeo_time;
                println!(">> tt CHECK A")
            } else if (zeo_time.saturating_sub(1)) & 0xFF == tt_lb as u32 {
                zeo_time_full = zeo_time.saturating_sub(1);
                println!(">> tt CHECK B {}", "=".repeat(10))
            } else if (zeo_time.saturating_add(1)) & 0xFF == tt_lb as u32 {
                zeo_time_full = zeo_time.saturating_add(1);
                println!(">> tt CHECK C {}", "=".repeat(10))
            } else {
                // Something doesn't line up. Maybe unit was reset.
                zeo_time_full = zeo_time;
                println!(">> tt CHECK D {}", "=".repeat(10))
            }

            // continue; // MAYBE?
        } else if datatype == DataType::Version {
            zeo_version = u32::from_le_bytes([
                datavec.pop_front().unwrap(),
                datavec.pop_front().unwrap(),
                datavec.pop_front().unwrap(),
                datavec.pop_front().unwrap_or(0),
            ]);
            println!("> zeo_version: {}", zeo_version);
            // continue; // MAYBE?
        }

        // MAYBE?
        // // Don't pass the timestamp or version data since we send that
        // // information along with the other data
        // if zeo_time == 0 || zeo_version == 0 {
        //     continue;
        // }

        println!("> zeo_time_full: {zeo_time_full} + {tt_ss} ({tt_fss:.02})");

        // for callback in self.callbacks:
        //     callback(zeo_time_full, timestamp_subsec, version, data)

        while_counter += 1;
    }

    // SUBSEC_SEEN.with(|c| {
    //     let c = c.borrow();
    //     println!["SUBSEC_SEEN: {:?}: {:?}", c.len(), c]
    // });

    Ok(Some((zeo_time_full, tt_ss, zeo_version, datatype, datavec)))
}
