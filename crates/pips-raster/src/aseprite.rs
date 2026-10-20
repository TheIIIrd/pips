//! Aseprite (`.aseprite`, `.ase`) input.
//!
//! Follows the published file spec (aseprite/docs/ase-file-specs.md). Every
//! frame is flattened the way Aseprite shows it: visible layers bottom to
//! top, normal blending, layer and cel opacity. What would look different
//! on a card is refused rather than approximated: other blend modes,
//! tilemaps, translucent or non-normal groups.
//!
//! Chunks not listed here (tags, slices, user data, colour profiles…) are
//! skipped by their declared size, which is still checked.

use alloc::vec;
use alloc::vec::Vec;

use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use crate::{check_size, Error, Frame, Raster, Rgba, MAX_PIXELS};

const FILE_MAGIC: u16 = 0xA5E0;
const FRAME_MAGIC: u16 = 0xF1FA;
const HEADER: usize = 128;

const CHUNK_OLD_PALETTE: u16 = 0x0004;
const CHUNK_OLD_PALETTE_6BIT: u16 = 0x0011;
const CHUNK_LAYER: u16 = 0x2004;
const CHUNK_CEL: u16 = 0x2005;
const CHUNK_PALETTE: u16 = 0x2019;

const LAYER_VISIBLE: u16 = 1;
const LAYER_BACKGROUND: u16 = 8;
const LAYER_REFERENCE: u16 = 64;

pub fn sniff(b: &[u8]) -> bool {
    b.len() >= 6 && u16::from_le_bytes([b[4], b[5]]) == FILE_MAGIC
}

/// A little-endian cursor that fails instead of reading past the end.
struct Rd<'a> {
    b: &'a [u8],
    at: usize,
}

const SHORT: Error = Error::Malformed("unexpected end of data");

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len()).ok_or(SHORT)?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, Error> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn i16(&mut self) -> Result<i16, Error> {
        let s = self.take(2)?;
        Ok(i16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, Error> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn len32(&mut self) -> Result<usize, Error> {
        usize::try_from(self.u32()?).map_err(|_| SHORT)
    }
}

struct Layer {
    flags: u16,
    kind: u16,
    level: u16,
    blend: u16,
    opacity: u8,
}

#[derive(Clone, Copy)]
struct Cel {
    layer: usize,
    x: i32,
    y: i32,
    opacity: u8,
    z: i32,
    w: usize,
    h: usize,
    /// Index into the image store.
    image: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Depth {
    Rgba,
    Gray,
    Indexed,
}

impl Depth {
    const fn bytes(self) -> usize {
        match self {
            Self::Rgba => 4,
            Self::Gray => 2,
            Self::Indexed => 1,
        }
    }
}

/// What the chunks have built so far.
struct State {
    depth: Depth,
    layer_opacity_valid: bool,
    layers: Vec<Layer>,
    palette: Vec<Rgba>,
    have_new_palette: bool,
    images: Vec<Vec<u8>>,
    image_bytes: usize,
    cels_by_frame: Vec<Vec<Cel>>,
}

pub fn decode(bytes: &[u8]) -> Result<Raster, Error> {
    let mut r = Rd { b: bytes, at: 0 };
    if r.len32()? != bytes.len() {
        return Err(Error::Malformed("file size in the header is wrong"));
    }
    r.u16()?; // magic, checked by sniff
    let frames = usize::from(r.u16()?);
    let (width, height) = (u32::from(r.u16()?), u32::from(r.u16()?));
    let depth = match r.u16()? {
        32 => Depth::Rgba,
        16 => Depth::Gray,
        8 => Depth::Indexed,
        _ => return Err(Error::Malformed("unknown colour depth")),
    };
    let header_flags = r.u32()?;
    r.take(2 + 4 + 4)?; // speed (deprecated), two reserved
    let transparent_index = r.u8()?;
    check_size(width, height, frames)?;
    r.at = HEADER;

    let mut st = State {
        depth,
        layer_opacity_valid: header_flags & 1 != 0,
        layers: Vec::new(),
        palette: Vec::new(),
        have_new_palette: false,
        images: Vec::new(),
        image_bytes: 0,
        cels_by_frame: Vec::with_capacity(frames),
    };
    let mut out = Vec::with_capacity(frames);
    for _ in 0..frames {
        let (cels, delay_ms) = st.frame(&mut r)?;
        let ctx = Ctx {
            width,
            height,
            depth,
            transparent_index,
            layers: &st.layers,
            palette: &st.palette,
            images: &st.images,
        };
        out.push(Frame { pixels: ctx.flatten(&cels)?, delay_ms });
        st.cels_by_frame.push(cels);
    }
    if r.at != bytes.len() {
        return Err(Error::Malformed("data after the last frame"));
    }
    Ok(Raster { width, height, frames: out })
}

impl State {
    /// Reads one frame: its chunks update the state, its cels are returned.
    fn frame(&mut self, r: &mut Rd<'_>) -> Result<(Vec<Cel>, u16), Error> {
        let start = r.at;
        let size = r.len32()?;
        let end = start.checked_add(size).filter(|&e| e <= r.b.len() && size >= 16).ok_or(SHORT)?;
        if r.u16()? != FRAME_MAGIC {
            return Err(Error::Malformed("bad frame magic"));
        }
        let old_count = r.u16()?;
        let delay_ms = r.u16()?;
        r.take(2)?;
        let new_count = r.u32()?;
        let chunks = if new_count == 0 { u32::from(old_count) } else { new_count };

        let mut cels = Vec::new();
        for _ in 0..chunks {
            let start = r.at;
            let size = r.len32()?;
            let chunk_end = start.checked_add(size).filter(|&e| e <= end && size >= 6).ok_or(SHORT)?;
            let kind = r.u16()?;
            // Each chunk reads from a view that ends where the chunk does.
            let mut c = Rd { b: &r.b[..chunk_end], at: r.at };
            match kind {
                CHUNK_LAYER => self.layer(&mut c)?,
                CHUNK_CEL => cels.push(self.cel(&mut c)?),
                CHUNK_PALETTE => self.new_palette(&mut c)?,
                CHUNK_OLD_PALETTE | CHUNK_OLD_PALETTE_6BIT if !self.have_new_palette => {
                    self.old_palette(&mut c, kind == CHUNK_OLD_PALETTE_6BIT)?;
                }
                _ => {}
            }
            r.at = chunk_end;
        }
        if r.at != end {
            return Err(Error::Malformed("frame size does not match its chunks"));
        }
        Ok((cels, delay_ms))
    }

    fn layer(&mut self, c: &mut Rd<'_>) -> Result<(), Error> {
        let flags = c.u16()?;
        let kind = c.u16()?;
        let level = c.u16()?;
        c.take(4)?; // default size, ignored by Aseprite too
        let blend = c.u16()?;
        let opacity = c.u8()?;
        let opacity = if self.layer_opacity_valid { opacity } else { 255 };
        self.layers.push(Layer { flags, kind, level, blend, opacity });
        Ok(())
    }

    #[allow(clippy::many_single_char_names)] // x, y, z, w, h: the spec's names
    fn cel(&mut self, c: &mut Rd<'_>) -> Result<Cel, Error> {
        let layer = usize::from(c.u16()?);
        let (x, y) = (i32::from(c.i16()?), i32::from(c.i16()?));
        let opacity = c.u8()?;
        let cel_type = c.u16()?;
        let z = i32::from(c.i16()?);
        c.take(5)?;
        match cel_type {
            0 | 2 => {
                let (w, h) = (usize::from(c.u16()?), usize::from(c.u16()?));
                let need = w.checked_mul(h).and_then(|n| n.checked_mul(self.depth.bytes())).ok_or(Error::TooLarge)?;
                self.image_bytes =
                    self.image_bytes.checked_add(need).filter(|&n| n <= MAX_PIXELS * 4).ok_or(Error::TooLarge)?;
                let rest = &c.b[c.at..];
                let data = if cel_type == 0 {
                    rest.get(..need).ok_or(SHORT)?.to_vec()
                } else {
                    decompress_to_vec_zlib_with_limit(rest, need)
                        .map_err(|_| Error::Malformed("cel pixels do not inflate"))?
                };
                if data.len() != need {
                    return Err(Error::Malformed("cel pixels do not match the cel size"));
                }
                self.images.push(data);
                Ok(Cel { layer, x, y, opacity, z, w, h, image: self.images.len() - 1 })
            }
            1 => {
                let linked = usize::from(c.u16()?);
                let src = self
                    .cels_by_frame
                    .get(linked)
                    .and_then(|f| f.iter().find(|k| k.layer == layer))
                    .ok_or(Error::Malformed("linked cel points nowhere"))?;
                Ok(Cel { x, y, opacity, z, ..*src })
            }
            3 => Err(Error::Unsupported("tilemap layers")),
            _ => Err(Error::Malformed("unknown cel type")),
        }
    }

    fn new_palette(&mut self, c: &mut Rd<'_>) -> Result<(), Error> {
        let size = c.len32()?;
        let (first, last) = (c.len32()?, c.len32()?);
        c.take(8)?;
        if size > 256 || first > last || last >= size {
            return Err(Error::Malformed("palette range"));
        }
        self.palette.resize(size, [0, 0, 0, 255]);
        for slot in &mut self.palette[first..=last] {
            let flags = c.u16()?;
            let p = c.take(4)?;
            *slot = [p[0], p[1], p[2], p[3]];
            if flags & 1 != 0 {
                let name = usize::from(c.u16()?);
                c.take(name)?;
            }
        }
        self.have_new_palette = true;
        Ok(())
    }

    fn old_palette(&mut self, c: &mut Rd<'_>, six_bit: bool) -> Result<(), Error> {
        // 0..=63 to 0..=255: 63 * 255 / 63 is exactly 255.
        let widen = |v: u8| if six_bit { u8::try_from(u16::from(v.min(63)) * 255 / 63).unwrap_or(255) } else { v };
        let packets = c.u16()?;
        let mut i = 0usize;
        for _ in 0..packets {
            i += usize::from(c.u8()?);
            let n = match c.u8()? {
                0 => 256,
                n => usize::from(n),
            };
            if i + n > 256 {
                return Err(Error::Malformed("palette range"));
            }
            if self.palette.len() < i + n {
                self.palette.resize(i + n, [0, 0, 0, 255]);
            }
            for slot in &mut self.palette[i..i + n] {
                let p = c.take(3)?;
                *slot = [widen(p[0]), widen(p[1]), widen(p[2]), 255];
            }
            i += n;
        }
        Ok(())
    }
}

struct Ctx<'a> {
    width: u32,
    height: u32,
    depth: Depth,
    transparent_index: u8,
    layers: &'a [Layer],
    palette: &'a [Rgba],
    images: &'a [Vec<u8>],
}

impl Ctx<'_> {
    /// Visible, normal-blended opacity for a layer, or `None` if hidden.
    fn layer_opacity(&self, i: usize) -> Result<Option<u8>, Error> {
        let l = self.layers.get(i).ok_or(Error::Malformed("cel on a missing layer"))?;
        if l.flags & LAYER_VISIBLE == 0 || l.flags & LAYER_REFERENCE != 0 {
            return Ok(None);
        }
        // Walk up the groups: a layer's parent is the nearest earlier layer
        // one level up.
        let mut level = l.level;
        for p in self.layers[..i].iter().rev() {
            if level == 0 {
                break;
            }
            if p.level == level - 1 {
                if p.flags & LAYER_VISIBLE == 0 {
                    return Ok(None);
                }
                // Files from before group opacity existed store 0 and
                // Aseprite ignores it; anything else would change the look.
                if p.blend != 0 || !matches!(p.opacity, 0 | 255) {
                    return Err(Error::Unsupported("groups with opacity or a blend mode"));
                }
                level -= 1;
            }
        }
        match l.kind {
            0 => {}
            1 => return Ok(None), // a group has no pixels of its own
            2 => return Err(Error::Unsupported("tilemap layers")),
            _ => return Err(Error::Malformed("unknown layer type")),
        }
        if l.blend != 0 {
            return Err(Error::Unsupported("blend modes other than normal"));
        }
        Ok(Some(l.opacity))
    }

    #[allow(clippy::many_single_char_names)] // x, y, w, h, c: geometry reads best short
    fn flatten(&self, cels: &[Cel]) -> Result<Vec<Rgba>, Error> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut canvas = vec![[0u8; 4]; w * h];
        let mut order: Vec<&Cel> = cels.iter().collect();
        // The spec's rule: order by layer + z, ties by z. A layer has one
        // cel per frame, so keys are unique.
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        crate::small_sort_by_key(&mut order, |c| (c.layer as i32 + c.z, c.z));
        for c in order {
            let Some(layer_op) = self.layer_opacity(c.layer)? else { continue };
            let background = self.layers[c.layer].flags & LAYER_BACKGROUND != 0;
            let op = mul(layer_op, c.opacity);
            let data = &self.images[c.image];
            for cy in 0..c.h {
                let Some(y) = coord(c.y, cy, h) else { continue };
                for cx in 0..c.w {
                    let Some(x) = coord(c.x, cx, w) else { continue };
                    let mut src = self.pixel(data, cy * c.w + cx, background)?;
                    src[3] = mul(src[3], op);
                    over(&mut canvas[y * w + x], src);
                }
            }
        }
        Ok(canvas)
    }

    fn pixel(&self, data: &[u8], i: usize, background: bool) -> Result<Rgba, Error> {
        Ok(match self.depth {
            Depth::Rgba => [data[i * 4], data[i * 4 + 1], data[i * 4 + 2], data[i * 4 + 3]],
            Depth::Gray => [data[i * 2], data[i * 2], data[i * 2], data[i * 2 + 1]],
            Depth::Indexed => {
                let k = data[i];
                if k == self.transparent_index && !background {
                    [0; 4]
                } else {
                    *self.palette.get(usize::from(k)).ok_or(Error::Malformed("colour index outside the palette"))?
                }
            }
        })
    }
}

/// Canvas coordinate for cel offset + position, if on the canvas.
fn coord(origin: i32, offset: usize, size: usize) -> Option<usize> {
    let v = i64::from(origin) + i64::try_from(offset).ok()?;
    usize::try_from(v).ok().filter(|&v| v < size)
}

/// a·b/255, rounded.
fn mul(a: u8, b: u8) -> u8 {
    let t = u32::from(a) * u32::from(b) + 128;
    u8::try_from((t + (t >> 8)) >> 8).unwrap_or(255)
}

/// Aseprite's own "normal" blend (`rgba_blender_normal`), integer for
/// integer: a textbook "source over" differs from it by one here and there,
/// and the card must match what the artist saw.
fn over(dst: &mut Rgba, src: Rgba) {
    if src[3] == 0 {
        return;
    }
    if dst[3] == 0 {
        *dst = src;
        return;
    }
    let (sa, ba) = (i32::from(src[3]), i32::from(dst[3]));
    let ra = sa + ba - i32::from(mul(dst[3], src[3]));
    for k in 0..3 {
        let (s, b) = (i32::from(src[k]), i32::from(dst[k]));
        // Rust's `/` truncates toward zero, as C's does.
        dst[k] = u8::try_from(b + (s - b) * sa / ra).unwrap_or(255);
    }
    dst[3] = u8::try_from(ra).unwrap_or(255);
}
