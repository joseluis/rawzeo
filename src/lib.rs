// rawzeo
//
/*!
A library to help deode the raw data protocol of the Zeo headband.

## Protocol specification
The serial port is set at baud 38400, no parity, one stop bit.
Data is sent Least Significant Byte first.

The serial protocol is: `AncllLLTttsidddd`, where:

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

*/
//

#![warn(clippy::all)]
#![allow(
    clippy::float_arithmetic,
    clippy::implicit_return,
    clippy::needless_return,
    clippy::blanket_clippy_restriction_lints,
    clippy::pattern_type_mismatch
)]
// #![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

use core::fmt;

/// All the types of events the base may send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DataType {
    /// An event has occured.
    Event = 0x00,

    /// Marks the end of a slice of data.
    // e.g.: C5 58 01 00
    SliceEnd = 0x02,

    /// Version of the raw data output.
    // e.g.: 03 00 00 00
    Version = 0x03,

    /// Raw time domain brainwave.
    // e.g.: datalen: 256:
    // 10 10 10 0F 20 0E 40 0D 40 0C 90 0B F0 0A 50 0A A0 09 F0 08 70 08 E0 07
    // 60 07 00 07 80 06 10 06 C0 05 60 05 10 05 C0 04 60 04 20 04 C0 03 E0 03
    // 10 03 10 04 30 01 30 0C 70 79 90 80 50 7F F0 7F A0 7F E0 7F C0 7F C0 7F
    // C0 7F C0 7F C0 7F F0 7F 80 7F 20 80 10 7F A0 80 20 77 30 B0 B0 3D 30 80
    // A0 A3 50 DF 60 6F 30 93 E0 7B F0 81 30 7F 00 80 00 80 F0 7F 00 80 00 80
    // 00 80 00 80 00 80 00 80 00 80 00 80 00 80 00 80 00 80 00 80 00 80 00 80
    // 00 80 00 80 00 80 00 80 00 80 00 80 00 80 00 80 00 80 F0 7F 10 80 C0 7F
    // 70 80 10 7F 40 82 F0 8A E0 7D 90 85 60 80 D0 7F 10 80 E0 7F 00 80 F0 7F
    // 00 80 00 80 00 80 00 80 F0 7F C0 7E 70 82 10 7B C0 89 70 6C 30 F3 10 93
    // F0 75 A0 84 40 7D F0 80 C0 7F C0 7F C0 7F C0 7F C0 7F C0 7F C0 7F C0 7F
    // C0 7F C0 7F C0 7F C0 7F C0 7F C0 7F C0 7F C0 7F
    Waveform = 0x80,

    /// Frequency bins derived from waveform.
    // e.g. datalen 14:
    // 29 1D 63 21 EE 1B 2D 11 B5 0C 31 0E 37 00
    FrequencyBins = 0x83,

    /// Signal Quality Index of waveform (0x00..=0x30).
    // e.g.: 09 00 00 00
    Sqi = 0x84,

    /// Timestamp from Zeo’s RTC.
    // e.g.:
    // 0B 26 B2 63
    // 0C 26 B2 63
    // 0D 26 B2 63
    ZeoTimestamp = 0x8A,

    /// Impedance across the headband.
    // e.g.:
    // A2 81 2A 83
    // FF FF 00 80
    Impedance = 0x97,

    /// Signal contains artifacts.
    // e.g.: 01 00 00 00
    BadSignal = 0x9C,

    /// Current 30sec sleep stage.
    SleepStage = 0x9D,

    /// Invalid data type.
    Invalid(u8) = 0xFF,
}
impl From<u8> for DataType {
    fn from(b: u8) -> DataType {
        use DataType::*;
        match b {
            0x00 => Event,
            0x02 => SliceEnd,
            0x03 => Version,
            0x80 => Waveform,
            0x83 => FrequencyBins,
            0x84 => Sqi,
            0x8A => ZeoTimestamp,
            0x97 => Impedance,
            0x9C => BadSignal,
            0x9D => SleepStage,
            _ => Invalid(b),
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use DataType::*;
        write!(
            f,
            "{}",
            match self {
                Event => "Event".into(),
                SliceEnd => "SliceEnd".into(),
                Version => "Version".into(),
                Waveform => "Waveform".into(),
                FrequencyBins => "FrequencyBins".into(),
                Sqi => "Sqi".into(),
                ZeoTimestamp => "ZeoTimestamp".into(),
                Impedance => "Impedance".into(),
                BadSignal => "BadSignal".into(),
                SleepStage => "SleepStage".into(),
                Invalid(b) => format!["Invalid({b})"],
            }
        )
    }
}

/// All the types of events that may be fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EventType {
    /// User's night has begun.
    NightStart = 0x05,

    /// User is asleep.
    SleepOnset = 0x07,

    /// Headband returned to dock.
    HeadbandDocked = 0x0E,

    /// Headband removed from dock.
    HeadbandUnDocked = 0x0F,

    /// User turned off the alarm.
    AlarmOff = 0x10,

    /// User hit snooze.
    AlarmSnooze = 0x11,

    /// Alarm is firing.
    AlarmPlay = 0x13,

    /// User = s night has ended.
    NightEnd = 0x15,

    /// A new headband ID has been read.
    NewHeadband = 0x24,

    /// Invalid event.
    Invalid(u8) = 0xFF,
}
impl From<u8> for EventType {
    fn from(b: u8) -> EventType {
        use EventType::*;
        match b {
            0x05 => NightStart,
            0x07 => SleepOnset,
            0x0E => HeadbandDocked,
            0x8F => HeadbandUnDocked,
            0x10 => AlarmOff,
            0x11 => AlarmSnooze,
            0x13 => AlarmPlay,
            0x15 => NightEnd,
            0x24 => NewHeadband,
            _ => Invalid(b),
        }
    }
}
impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use EventType::*;
        write!(
            f,
            "{}",
            match self {
                NightStart => "NightStart".into(),
                SleepOnset => "SleepOnset".into(),
                HeadbandDocked => "HeadbandDocked".into(),
                HeadbandUnDocked => "HeadbandUnDocked".into(),
                AlarmOff => "AlarmOff".into(),
                AlarmSnooze => "AlarmSnooze".into(),
                AlarmPlay => "AlarmPlay".into(),
                NightEnd => "NightEnd".into(),
                NewHeadband => "NewHeadband".into(),
                Invalid(b) => format!["Invalid({b})"],
            }
        )
    }
}

/// All the frequency bins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FrequencyBins {
    /// Delta (2-4 Hz).
    Delta = 0x00,

    /// Theta (4-8 Hz).
    Theta = 0x01,

    /// Alpha (8-13 Hz).
    Alpha = 0x02,

    /// Beta mid range (11-14 Hz).
    BetaMid = 0x03,

    /// Beta high (18-21 Hz).
    BetaHigh = 0x04,

    /// Beta low (sleep spindles) (11-14 Hz).
    BetaLow = 0x05,

    /// Gamma.
    Gamma = 0x06,

    /// Invalid frequency bin.
    Invalid(u8) = 0xFF,
}
impl FrequencyBins {
    /// Returns the interval of frequencies of this frequency bin (min, max).
    //
    // IMPROVE: interval type? (numera? ladata? devela?)
    pub fn hz(&self) -> (u8, u8) {
        use FrequencyBins::*;
        match self {
            Delta => (2, 4),
            Theta => (4, 8),
            Alpha => (8, 13),
            BetaLow => (11, 14),
            BetaMid => (13, 18),
            BetaHigh => (18, 21),
            Gamma => (30, 50),
            Invalid(_) => (0, 0),
        }
    }
    pub fn is_delta(&self) -> bool {
        matches![self, FrequencyBins::Delta]
    }
    pub fn is_theta(&self) -> bool {
        matches![self, FrequencyBins::Theta]
    }
    pub fn is_alpha(&self) -> bool {
        matches![self, FrequencyBins::Alpha]
    }
    pub fn is_gamma(&self) -> bool {
        matches![self, FrequencyBins::Gamma]
    }
    pub fn is_betta(&self) -> bool {
        use FrequencyBins::*;
        matches![self, BetaLow | BetaMid | BetaHigh]
    }
}
impl From<u8> for FrequencyBins {
    fn from(b: u8) -> FrequencyBins {
        use FrequencyBins::*;
        match b {
            0x00 => Delta,
            0x01 => Theta,
            0x02 => Alpha,
            0x03 => BetaMid,
            0x04 => BetaHigh,
            0x05 => BetaLow,
            0x06 => Gamma,
            _ => Invalid(b),
        }
    }
}
impl fmt::Display for FrequencyBins {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use FrequencyBins::*;
        write!(
            f,
            "{}",
            match self {
                Delta => "Delta".into(),
                Theta => "Theta".into(),
                Alpha => "Alpha".into(),
                BetaMid => "BetaMid".into(),
                BetaHigh => "BetaHigh".into(),
                BetaLow => "BetaLow".into(),
                Gamma => "Gamma".into(),
                Invalid(b) => format!["Invalid({b})"],
            }
        )
    }
}

/// The sleep stages output by the base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SleepStages {
    /// Sleeps tage unsure.
    Undefined = 0x00,

    /// Awake.
    Awake = 0x01,

    /// Rapid eye movement (possibly dreaming).
    Rem = 0x02,

    /// Light sleep.
    Light = 0x03,

    /// Deep sleep.
    Deep = 0x04,

    /// Invalid sleep stage.
    Invalid(u8) = 0xFF,
}
impl From<u8> for SleepStages {
    fn from(b: u8) -> SleepStages {
        use SleepStages::*;
        match b {
            0x00 => Undefined,
            0x01 => Awake,
            0x02 => Rem,
            0x03 => Light,
            0x04 => Deep,
            _ => Invalid(b),
        }
    }
}
impl fmt::Display for SleepStages {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use SleepStages::*;
        write!(
            f,
            "{}",
            match self {
                Undefined => "Undefined".into(),
                Awake => "Awake".into(),
                Rem => "Rem".into(),
                Light => "Light".into(),
                Deep => "Deep".into(),
                Invalid(b) => format!["Invalid({b})"],
            }
        )
    }
}

/// Filters out 60hz noise from a signal.
/// In practice it is a sinc low pass filter with cutoff frequency of 50hz.
// fn filter_60hz<const LEN: >(a: [8; 256]) {
pub fn filter60hz(a: &[f64]) -> Vec<f64> {
    // Filter designed in matlab
    let filter: [f64; 51] = [
        0.0056, 0.0190, 0.0113, -0.0106, 0.0029, 0.0041, -0.0082, 0.0089, -0.0062, 0.0006, 0.0066,
        -0.0129, 0.0157, -0.0127, 0.0035, 0.0102, -0.0244, 0.0336, -0.0323, 0.0168, 0.0136,
        -0.0555, 0.1020, -0.1446, 0.1743, 0.8150, 0.1743, -0.1446, 0.1020, -0.0555, 0.0136, 0.0168,
        -0.0323, 0.0336, -0.0244, 0.0102, 0.0035, -0.0127, 0.0157, -0.0129, 0.0066, 0.0006,
        -0.0062, 0.0089, -0.0082, 0.0041, 0.0029, -0.0106, 0.0113, 0.0190, 0.0056,
    ];
    // Convolution math from http://web.archive.org/web/20100528145622/http://www.phys.uu.nl/~haque/computing/WPark_recipes_in_python.html
    let p = a.len();
    let q = filter.len();
    let n = p + q - 1;
    let mut c = vec![0.0; n];
    for k in 0..n {
        let mut t = 0.0;
        let lower = k.saturating_sub(q - 1);
        let upper = k.min(p - 1);
        for i in lower..=upper {
            t += a[i] * filter[k - i];
        }
        c[k] = t;
    }
    c

    // P = len(A)
    // Q = len(filter)
    // N = P + Q - 1
    // c = []
    // for k in range(N):
    //     t = 0
    //     lower = max(0, k-(Q-1))
    //     upper = min(P-1, k)
    //     for i in range(lower, upper+1):
    //         t = t + A[i] * filter[k-i]
    //     c.append(t)
    // return c
}
