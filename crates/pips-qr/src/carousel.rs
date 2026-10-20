//! A fountain-coded carousel: a message too big for one QR code, shown as a
//! loop of codes that a camera can join in on at any point.
//!
//! The message is cut into `n` equal fragments. Frame `seq < n` carries
//! fragment `seq` as is; every later frame carries the XOR of a
//! pseudo-random half of the fragments, chosen by its `seq`. Any `n`
//! independent frames determine the message, so a camera that misses frames
//! — or starts halfway through — just keeps looking. With random halves,
//! `n + k` frames suffice with probability about `1 - 2^-k`.
//!
//! The decoder solves the frames as a linear system over GF(2) by
//! incremental Gaussian elimination: each frame costs at most `n` XORs of a
//! fragment, and at most `n` frames are ever stored.
//!
//! # Frame layout
//!
//! ```text
//! kind      u8     'F'
//! version   u8     1
//! msg_len   u16    1..=65535
//! frag_len  u16    16..=1024; n = ceil(msg_len / frag_len) <= 256
//! check     u32    CRC-32 of the message
//! seq       u32    frame number
//! data      frag_len bytes
//! ```
//!
//! All little-endian. The last fragment is zero-padded.

use alloc::vec::Vec;
use core::fmt;

/// First byte of a carousel frame.
pub const KIND: u8 = b'F';
const VERSION: u8 = 1;
const HEADER_LEN: usize = 14;
/// Most fragments a message may be cut into.
pub const MAX_FRAGMENTS: usize = 256;
/// Fragment size bounds.
pub const FRAGMENT_LEN: core::ops::RangeInclusive<usize> = 16..=1024;

type Mask = [u64; MAX_FRAGMENTS / 64];

/// Why a frame was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrameError {
    /// Shorter than its header says.
    Truncated,
    /// Not a carousel frame.
    WrongKind,
    /// A version this crate does not read.
    UnsupportedVersion(u8),
    /// Lengths out of range, or data not exactly one fragment long.
    BadHeader,
    /// From a different message than the frames before it. Start a new
    /// [`Decoder`] to switch.
    Mismatch,
    /// All fragments solved, but the result does not match the checksum:
    /// a frame was corrupted. The decoder resets.
    Checksum,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Truncated => f.write_str("truncated frame"),
            Self::WrongKind => f.write_str("not a carousel frame"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported carousel version {v}"),
            Self::BadHeader => f.write_str("malformed carousel frame"),
            Self::Mismatch => f.write_str("frame belongs to another message"),
            Self::Checksum => f.write_str("message checksum failed; starting over"),
        }
    }
}

impl core::error::Error for FrameError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Header {
    msg_len: usize,
    frag_len: usize,
    check: u32,
}

impl Header {
    fn fragments(&self) -> usize {
        self.msg_len.div_ceil(self.frag_len)
    }
}

/// Produces frames for one message.
#[derive(Debug, Clone)]
pub struct Encoder {
    header: Header,
    padded: Vec<u8>,
}

/// Why a message cannot be put on a carousel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge;

impl fmt::Display for TooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("message is empty or does not fit the carousel limits")
    }
}

impl core::error::Error for TooLarge {}

impl Encoder {
    /// An encoder cutting `message` into fragments of `frag_len` bytes.
    ///
    /// # Errors
    ///
    /// [`TooLarge`] for an empty message, one over 65535 bytes, a fragment
    /// size outside [`FRAGMENT_LEN`], or more than [`MAX_FRAGMENTS`] fragments.
    pub fn new(message: &[u8], frag_len: usize) -> Result<Self, TooLarge> {
        if message.is_empty() || message.len() > usize::from(u16::MAX) || !FRAGMENT_LEN.contains(&frag_len) {
            return Err(TooLarge);
        }
        let header = Header { msg_len: message.len(), frag_len, check: crc32(message) };
        if header.fragments() > MAX_FRAGMENTS {
            return Err(TooLarge);
        }
        let mut padded = message.to_vec();
        padded.resize(header.fragments() * frag_len, 0);
        Ok(Self { header, padded })
    }

    /// Number of fragments, and the fewest frames that can carry the message.
    #[must_use]
    pub fn fragments(&self) -> usize {
        self.header.fragments()
    }

    /// Frame number `seq`. Any `seq` is valid; the sequence never ends.
    #[must_use]
    pub fn frame(&self, seq: u32) -> Vec<u8> {
        let Header { msg_len, frag_len, check } = self.header;
        let mut out = Vec::with_capacity(HEADER_LEN + frag_len);
        out.extend_from_slice(&[KIND, VERSION]);
        // Both checked in `new` to fit in 16 bits.
        #[allow(clippy::cast_possible_truncation)]
        {
            out.extend_from_slice(&(msg_len as u16).to_le_bytes());
            out.extend_from_slice(&(frag_len as u16).to_le_bytes());
        }
        out.extend_from_slice(&check.to_le_bytes());
        out.extend_from_slice(&seq.to_le_bytes());
        let mut data = alloc::vec![0u8; frag_len];
        let mask = coefficients(seq, self.fragments(), check);
        for i in 0..self.fragments() {
            if bit(&mask, i) {
                xor(&mut data, &self.padded[i * frag_len..(i + 1) * frag_len]);
            }
        }
        out.extend_from_slice(&data);
        out
    }
}

/// Where a [`Decoder`] stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// `have` independent frames of the `need` required.
    Partial {
        /// Useful frames so far.
        have: usize,
        /// Frames needed in total.
        need: usize,
    },
    /// The whole message, checksum verified.
    Done(Vec<u8>),
}

/// Joins frames back into the message.
#[derive(Debug, Default)]
pub struct Decoder {
    header: Option<Header>,
    // Row with lowest set bit `i` lives at `basis[i]`.
    basis: Vec<Option<(Mask, Vec<u8>)>>,
    rank: usize,
}

impl Decoder {
    /// An empty decoder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes one frame. Duplicates and frames that add nothing are accepted
    /// and ignored.
    ///
    /// # Errors
    ///
    /// A [`FrameError`]; a malformed or foreign frame leaves the decoder as
    /// it was, a checksum failure resets it.
    pub fn push(&mut self, frame: &[u8]) -> Result<Progress, FrameError> {
        let (header, seq, data) = parse(frame)?;
        match self.header {
            None => {
                self.header = Some(header);
                self.basis = alloc::vec![None; header.fragments()];
            }
            Some(h) if h != header => return Err(FrameError::Mismatch),
            Some(_) => {}
        }
        let n = header.fragments();
        let mut mask = coefficients(seq, n, header.check);
        let mut row = data.to_vec();
        for i in 0..n {
            if !bit(&mask, i) {
                continue;
            }
            if let Some((m, d)) = &self.basis[i] {
                xor_mask(&mut mask, m);
                xor(&mut row, d);
            } else {
                self.basis[i] = Some((mask, row));
                self.rank += 1;
                break;
            }
        }
        if self.rank < n {
            return Ok(Progress::Partial { have: self.rank, need: n });
        }
        self.solve(header)
    }

    /// Back-substitution once every pivot is present.
    fn solve(&mut self, header: Header) -> Result<Progress, FrameError> {
        let n = header.fragments();
        for i in (0..n).rev() {
            for j in i + 1..n {
                let needs = self.basis[i].as_ref().is_some_and(|(m, _)| bit(m, j));
                if needs {
                    let (mj, dj) = self.basis[j].clone().expect("full rank");
                    let (mi, di) = self.basis[i].as_mut().expect("full rank");
                    xor_mask(mi, &mj);
                    xor(di, &dj);
                }
            }
        }
        let mut message = Vec::with_capacity(n * header.frag_len);
        for row in &self.basis {
            message.extend_from_slice(&row.as_ref().expect("full rank").1);
        }
        message.truncate(header.msg_len);
        if crc32(&message) != header.check {
            *self = Self::new();
            return Err(FrameError::Checksum);
        }
        Ok(Progress::Done(message))
    }
}

fn parse(frame: &[u8]) -> Result<(Header, u32, &[u8]), FrameError> {
    if frame.first() != Some(&KIND) {
        return Err(if frame.is_empty() { FrameError::Truncated } else { FrameError::WrongKind });
    }
    if frame.len() < HEADER_LEN {
        return Err(FrameError::Truncated);
    }
    if frame[1] != VERSION {
        return Err(FrameError::UnsupportedVersion(frame[1]));
    }
    let u16_at = |i: usize| usize::from(u16::from_le_bytes([frame[i], frame[i + 1]]));
    let u32_at = |i: usize| u32::from_le_bytes([frame[i], frame[i + 1], frame[i + 2], frame[i + 3]]);
    let header = Header { msg_len: u16_at(2), frag_len: u16_at(4), check: u32_at(6) };
    let seq = u32_at(10);
    if header.msg_len == 0 || !FRAGMENT_LEN.contains(&header.frag_len) || header.fragments() > MAX_FRAGMENTS {
        return Err(FrameError::BadHeader);
    }
    let data = &frame[HEADER_LEN..];
    if data.len() < header.frag_len {
        return Err(FrameError::Truncated);
    }
    if data.len() > header.frag_len {
        return Err(FrameError::BadHeader);
    }
    Ok((header, seq, data))
}

/// Which fragments frame `seq` combines. Part of the format: both ends must
/// compute exactly this.
fn coefficients(seq: u32, n: usize, check: u32) -> Mask {
    let mut mask = [0u64; 4];
    let seq_index = usize::try_from(seq).unwrap_or(usize::MAX);
    if seq_index < n {
        set(&mut mask, seq_index);
        return mask;
    }
    let mut state = u64::from(check) << 32 | u64::from(seq);
    for word in mask.iter_mut().take(n.div_ceil(64)) {
        *word = splitmix64(&mut state);
    }
    // Clear bits past `n`.
    if n % 64 != 0 {
        mask[n / 64] &= (1u64 << (n % 64)) - 1;
    }
    if mask.iter().all(|&w| w == 0) {
        set(&mut mask, seq_index % n);
    }
    mask
}

/// `SplitMix64` (Steele, Lea, Flood 2014): tiny, and specified exactly.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn bit(m: &Mask, i: usize) -> bool {
    m[i / 64] >> (i % 64) & 1 == 1
}

fn set(m: &mut Mask, i: usize) {
    m[i / 64] |= 1 << (i % 64);
}

fn xor_mask(a: &mut Mask, b: &Mask) {
    for (x, y) in a.iter_mut().zip(b) {
        *x ^= y;
    }
}

fn xor(a: &mut [u8], b: &[u8]) {
    for (x, y) in a.iter_mut().zip(b) {
        *x ^= y;
    }
}

/// CRC-32 (IEEE 802.3, as in zlib and PNG).
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}
