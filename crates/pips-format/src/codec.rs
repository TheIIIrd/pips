//! Byte layout: see the crate docs.

use alloc::string::String;
use alloc::vec::Vec;

use miniz_oxide::inflate::stream::{inflate, InflateState};
use miniz_oxide::{DataFormat, MZFlush, MZStatus};
use sha2::{Digest as _, Sha256};

use crate::flags::{ANIMATED, CRITICAL, OPAQUE, SIGNED};
use crate::limits::{MAX_AUTHOR, MAX_EXTENSIONS, MAX_EXT_DATA, MAX_HANDLE, MAX_LINK, MAX_PAYLOAD, MAX_TITLE};
use crate::{validate, Animation, Card, Error, Extension, Field, Handle, Network, Palette, PaletteRef, Signature};
use crate::{MAGIC, VERSION};

const PALETTE_INLINE: u8 = 0xFF;
const KNOWN_FLAGS: u8 = OPAQUE | ANIMATED | SIGNED;
/// Domain separation for [`digest`]; signatures use their own prefix.
const DIGEST_DOMAIN: &[u8] = b"pips-v1:digest";

// ------------------------------------------------------------------ write --

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    // Every caller has validated the length against a limit below 256.
    #[allow(clippy::cast_possible_truncation)]
    out.push(b.len() as u8);
    out.extend_from_slice(b);
}

fn write_header(c: &Card, out: &mut Vec<u8>, canonical: bool) {
    let mut flags = c.unknown_flags;
    if c.opaque {
        flags |= OPAQUE;
    }
    if c.animation.is_some() {
        flags |= ANIMATED;
    }
    if !canonical && c.signature.is_some() {
        flags |= SIGNED;
    }
    out.extend_from_slice(&[VERSION, flags, c.width, c.height]);
    match c.palette {
        PaletteRef::Builtin(id) => out.push(id),
        PaletteRef::Inline(p) => {
            out.push(PALETTE_INLINE);
            #[allow(clippy::cast_possible_truncation)] // at most 32
            out.push(p.len() as u8);
            for col in p.colors() {
                out.extend_from_slice(&[col.r, col.g, col.b]);
            }
        }
    }
    out.push(c.frame);
    if let Some(a) = c.animation {
        out.push(a.frames);
        out.extend_from_slice(&a.delay_ms.to_le_bytes());
    }
    put_bytes(out, c.title.as_bytes());
    put_bytes(out, c.author.as_bytes());
    match &c.handle {
        None => out.push(0),
        Some(h) => {
            out.push(h.network as u8);
            put_bytes(out, h.text.as_bytes());
        }
    }
    put_bytes(out, c.link.as_bytes());
    #[allow(clippy::cast_possible_truncation)] // at most 16
    out.push(c.extensions.len() as u8);
    for e in &c.extensions {
        #[allow(clippy::cast_possible_truncation)] // tag at most 24
        out.push(u8::from(e.critical) << 7 | e.tag.len() as u8);
        out.extend_from_slice(e.tag.as_bytes());
        #[allow(clippy::cast_possible_truncation)] // data at most 4096
        out.extend_from_slice(&(e.data.len() as u16).to_le_bytes());
        out.extend_from_slice(&e.data);
    }
}

/// Encodes a card as a bare payload, the form a QR code carries.
///
/// # Errors
///
/// Any rule the decoder would refuse the result for.
pub fn encode(card: &Card) -> Result<Vec<u8>, Error> {
    validate::card(card)?;
    let mut out = Vec::with_capacity(128);
    write_header(card, &mut out, false);
    let packed = miniz_oxide::deflate::compress_to_vec(&card.pixels, 10);
    let len = u16::try_from(packed.len()).map_err(|_| Error::PayloadTooLarge)?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&packed);
    if let Some(s) = &card.signature {
        out.extend_from_slice(&s.public_key);
        out.extend_from_slice(&s.bytes);
    }
    if out.len() > MAX_PAYLOAD {
        return Err(Error::PayloadTooLarge);
    }
    Ok(out)
}

/// Encodes a card as a `.pips` file: [`MAGIC`], then the payload.
///
/// # Errors
///
/// As [`encode`].
pub fn encode_file(card: &Card) -> Result<Vec<u8>, Error> {
    let payload = encode(card)?;
    let mut out = Vec::with_capacity(MAGIC.len() + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&payload);
    Ok(out)
}

/// The canonical content form: the header without the signed flag, then
/// the raw pixels behind a `u32` length. Signatures cover it; [`digest`]
/// hashes it. It is the same whichever deflate implementation wrote the
/// payload.
///
/// # Errors
///
/// As [`encode`].
pub fn canonical_bytes(card: &Card) -> Result<Vec<u8>, Error> {
    validate::card(card)?;
    let mut out = Vec::with_capacity(64 + card.pixels.len());
    write_header(card, &mut out, true);
    let n = u32::try_from(card.pixels.len()).map_err(|_| Error::TooManyPixels)?;
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&card.pixels);
    Ok(out)
}

/// `SHA-256("pips-v1:digest" || canonical form)`: the card's identity,
/// stable across encoders, unaffected by the signature.
///
/// # Errors
///
/// As [`encode`].
pub fn digest(card: &Card) -> Result<[u8; 32], Error> {
    let body = canonical_bytes(card)?;
    let mut h = Sha256::new();
    h.update(DIGEST_DOMAIN);
    h.update(&body);
    Ok(h.finalize().into())
}

// ------------------------------------------------------------------- read --

struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if n > self.rest.len() {
            return Err(Error::Truncated);
        }
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, Error> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut a = [0; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    /// A length-prefixed string; the length is checked before the bytes
    /// are read.
    fn string(&mut self, max: usize, err: Error) -> Result<String, Error> {
        let n = usize::from(self.u8()?);
        if n > max {
            return Err(err);
        }
        let b = self.take(n)?;
        core::str::from_utf8(b).map(String::from).map_err(|_| err)
    }
}

/// Decodes a bare payload. Every input is treated as hostile; see the
/// crate docs.
///
/// # Errors
///
/// The first rule the input breaks.
pub fn decode(bytes: &[u8]) -> Result<Card, Error> {
    if bytes.len() > MAX_PAYLOAD {
        return Err(Error::PayloadTooLarge);
    }
    let mut r = Reader { rest: bytes };

    let version = r.u8()?;
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let flags = r.u8()?;
    let unknown_critical = flags & CRITICAL & !KNOWN_FLAGS;
    if unknown_critical != 0 {
        return Err(Error::UnknownCriticalFlag(unknown_critical));
    }
    let width = r.u8()?;
    let height = r.u8()?;
    if width == 0 || height == 0 || width > crate::limits::MAX_EDGE || height > crate::limits::MAX_EDGE {
        return Err(Error::BadDimensions);
    }

    let palette = read_palette(&mut r)?;
    let frame = r.u8()?;
    if frame > crate::limits::MAX_FRAME_SHADE {
        return Err(Error::BadFrame);
    }
    let animation = if flags & ANIMATED != 0 {
        let a = Animation { frames: r.u8()?, delay_ms: r.u16()? };
        if a.frames < 2 || a.frames > crate::limits::MAX_FRAMES || !crate::limits::DELAY_MS.contains(&a.delay_ms) {
            return Err(Error::BadAnimation);
        }
        Some(a)
    } else {
        None
    };

    let title = r.string(MAX_TITLE, Error::BadText(Field::Title))?;
    let author = r.string(MAX_AUTHOR, Error::BadText(Field::Author))?;
    let handle = match r.u8()? {
        0 => None,
        b => {
            let network = Network::from_byte(b).ok_or(Error::BadHandle)?;
            Some(Handle { network, text: r.string(MAX_HANDLE, Error::BadHandle)? })
        }
    };
    let link = r.string(MAX_LINK, Error::BadLink)?;

    let extensions = read_extensions(&mut r)?;

    let frames = animation.map_or(1, |a| a.frames);
    let expected = validate::pixel_count(width, height, frames)?;
    let packed_len = usize::from(r.u16()?);
    let pixels = inflate_exact(r.take(packed_len)?, expected)?;

    let signature =
        if flags & SIGNED != 0 { Some(Signature { public_key: r.array()?, bytes: r.array()? }) } else { None };
    if !r.rest.is_empty() {
        return Err(Error::TrailingBytes);
    }

    let card = Card {
        width,
        height,
        opaque: flags & OPAQUE != 0,
        palette,
        frame,
        animation,
        title,
        author,
        handle,
        link,
        extensions,
        unknown_flags: flags & !KNOWN_FLAGS,
        pixels,
        signature,
    };
    // One set of field rules for both directions: whatever decodes, encodes.
    validate::card(&card)?;
    Ok(card)
}

fn read_palette(r: &mut Reader<'_>) -> Result<PaletteRef, Error> {
    Ok(match r.u8()? {
        PALETTE_INLINE => {
            let n = usize::from(r.u8()?);
            if !(pips_palette::MIN_COLORS..=pips_palette::MAX_COLORS).contains(&n) {
                return Err(Error::BadPaletteSize);
            }
            let raw = r.take(n * 3)?;
            let mut colors = [pips_palette::Rgb::default(); pips_palette::MAX_COLORS];
            for (c, rgb) in colors.iter_mut().zip(raw.chunks_exact(3)) {
                *c = pips_palette::Rgb::new(rgb[0], rgb[1], rgb[2]);
            }
            PaletteRef::Inline(Palette::new(&colors[..n]).map_err(|_| Error::BadPaletteSize)?)
        }
        id => {
            let p = PaletteRef::Builtin(id);
            p.resolve().ok_or(Error::UnknownPalette(id))?;
            p
        }
    })
}

fn read_extensions(r: &mut Reader<'_>) -> Result<Vec<Extension>, Error> {
    let count = usize::from(r.u8()?);
    if count > MAX_EXTENSIONS {
        return Err(Error::BadExtension);
    }
    let mut extensions = Vec::with_capacity(count);
    for _ in 0..count {
        let head = r.u8()?;
        let tag_len = usize::from(head & 0x7F);
        let tag = core::str::from_utf8(r.take(tag_len)?).map_err(|_| Error::BadExtension)?;
        if !validate::ext_tag(tag) {
            return Err(Error::BadExtension);
        }
        let data_len = usize::from(r.u16()?);
        if data_len > MAX_EXT_DATA {
            return Err(Error::BadExtension);
        }
        let data = r.take(data_len)?.to_vec();
        extensions.push(Extension { critical: head & 0x80 != 0, tag: String::from(tag), data });
    }
    Ok(extensions)
}

/// Decodes a `.pips` file.
///
/// # Errors
///
/// [`Error::BadMagic`], or as [`decode`].
pub fn decode_file(bytes: &[u8]) -> Result<Card, Error> {
    decode(bytes.strip_prefix(&MAGIC[..]).ok_or(Error::BadMagic)?)
}

/// Inflates a raw deflate stream that must produce exactly `expected` bytes
/// and consume all of `input`. The output buffer is the bound: one byte of
/// slack detects a stream that would run long, so a decompression bomb
/// costs at most `expected + 1` bytes.
fn inflate_exact(input: &[u8], expected: usize) -> Result<Vec<u8>, Error> {
    let mut out = alloc::vec![0u8; expected + 1];
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let res = inflate(&mut state, input, &mut out, MZFlush::Finish);
    match res.status {
        Ok(MZStatus::StreamEnd) if res.bytes_consumed == input.len() && res.bytes_written == expected => {
            out.truncate(expected);
            Ok(out)
        }
        _ => Err(Error::CorruptPixels),
    }
}
