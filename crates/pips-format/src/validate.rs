//! Field rules shared by the encoder and the decoder, so that anything one
//! accepts the other produces, and nothing else.

use crate::limits::{MAX_EXTENSIONS, MAX_EXT_DATA, MAX_EXT_TAG};
use crate::{Card, Error, Extension, Field};

/// Printable text without control characters or surrounding whitespace.
pub fn text(s: &str, max: usize, field: Field) -> Result<(), Error> {
    let ok = s.len() <= max && !s.chars().any(char::is_control) && s.trim() == s;
    if ok {
        Ok(())
    } else {
        Err(Error::BadText(field))
    }
}

pub fn handle(s: &str) -> Result<(), Error> {
    let ok = !s.is_empty()
        && s.len() <= crate::limits::MAX_HANDLE
        && !s.chars().any(|c| c.is_control() || c.is_whitespace());
    if ok {
        Ok(())
    } else {
        Err(Error::BadHandle)
    }
}

/// `host[/rest]`, where the scheme is implicitly https.
///
/// The host is a lowercase DNS name with at least two labels and a
/// non-numeric top label, so IP addresses, `localhost`, userinfo
/// (`a@b`), ports and other schemes (`javascript:`) cannot be expressed.
/// The rest is printable ASCII minus characters that need escaping.
pub fn link(s: &str) -> Result<(), Error> {
    if s.is_empty() {
        return Ok(());
    }
    if s.len() > crate::limits::MAX_LINK {
        return Err(Error::BadLink);
    }
    let (host, rest) = s.find(['/', '?', '#']).map_or((s, ""), |i| s.split_at(i));
    let labels_ok = host.split('.').all(|l| {
        !l.is_empty()
            && l.len() <= 63
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    });
    let top_ok = host.rsplit('.').next().is_some_and(|t| !t.bytes().all(|b| b.is_ascii_digit()));
    let rest_ok = rest.bytes().all(|b| (0x21..=0x7E).contains(&b) && !b"\"<>\\^`{|}".contains(&b));
    if host.contains('.') && labels_ok && top_ok && rest_ok {
        Ok(())
    } else {
        Err(Error::BadLink)
    }
}

pub fn ext_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= MAX_EXT_TAG
        && tag.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
}

/// This version knows no extensions, so every critical one is refused.
pub fn extensions(exts: &[Extension]) -> Result<(), Error> {
    if exts.len() > MAX_EXTENSIONS {
        return Err(Error::BadExtension);
    }
    for e in exts {
        if !ext_tag(&e.tag) || e.data.len() > MAX_EXT_DATA {
            return Err(Error::BadExtension);
        }
        if e.critical {
            return Err(Error::UnknownCriticalExtension);
        }
    }
    if exts.windows(2).any(|w| w[0].tag >= w[1].tag) {
        return Err(Error::ExtensionOrder);
    }
    Ok(())
}

/// Every rule on a whole card except the pixel stream encoding itself.
pub fn card(c: &Card) -> Result<(), Error> {
    use crate::flags::CRITICAL;
    use crate::limits::{DELAY_MS, MAX_AUTHOR, MAX_EDGE, MAX_FRAMES, MAX_FRAME_SHADE, MAX_TITLE};

    if c.width == 0 || c.height == 0 || c.width > MAX_EDGE || c.height > MAX_EDGE {
        return Err(Error::BadDimensions);
    }
    if c.unknown_flags & CRITICAL != 0 {
        return Err(Error::UnknownCriticalFlag(c.unknown_flags & CRITICAL));
    }
    let palette = match c.palette {
        crate::PaletteRef::Builtin(id) => c.palette.resolve().ok_or(Error::UnknownPalette(id))?,
        crate::PaletteRef::Inline(p) => p,
    };
    if c.frame > MAX_FRAME_SHADE {
        return Err(Error::BadFrame);
    }
    if let Some(a) = c.animation {
        if a.frames < 2 || a.frames > MAX_FRAMES || !DELAY_MS.contains(&a.delay_ms) {
            return Err(Error::BadAnimation);
        }
    }
    text(&c.title, MAX_TITLE, Field::Title)?;
    text(&c.author, MAX_AUTHOR, Field::Author)?;
    if let Some(h) = &c.handle {
        handle(&h.text)?;
    }
    link(&c.link)?;
    extensions(&c.extensions)?;
    let expected = pixel_count(c.width, c.height, c.frames())?;
    if c.pixels.len() != expected {
        return Err(Error::PixelCount);
    }
    check_indices(&c.pixels, palette.len())?;
    Ok(())
}

/// `width * height * frames`, refused above the limit before anything is
/// allocated for it.
pub fn pixel_count(width: u8, height: u8, frames: u8) -> Result<usize, Error> {
    let n = usize::from(width) * usize::from(height) * usize::from(frames);
    if n > crate::limits::MAX_PIXELS {
        Err(Error::TooManyPixels)
    } else {
        Ok(n)
    }
}

pub fn check_indices(pixels: &[u8], colors: usize) -> Result<(), Error> {
    if pixels.iter().any(|&p| usize::from(p) >= colors) {
        Err(Error::PixelOutOfPalette)
    } else {
        Ok(())
    }
}
