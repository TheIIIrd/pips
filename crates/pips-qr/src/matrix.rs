//! Drawing QR symbols.

use std::fmt::Write as _;

use qrcodegen::{QrCode, QrCodeEcc, QrSegment};

/// A QR symbol: a square of dark and light modules, without quiet zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matrix {
    size: usize,
    dark: Vec<bool>,
}

/// Text that does not fit even the largest QR symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLong;

impl std::fmt::Display for TooLong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("text does not fit in a QR code")
    }
}

impl std::error::Error for TooLong {}

impl Matrix {
    /// Encodes `text` at error-correction level M or higher (raised for free
    /// when the symbol has room). Base45 text uses alphanumeric mode; other
    /// text falls back to byte mode.
    ///
    /// # Errors
    ///
    /// [`TooLong`].
    pub fn encode(text: &str) -> Result<Self, TooLong> {
        let seg = if QrSegment::is_alphanumeric(text) {
            QrSegment::make_alphanumeric(text)
        } else {
            QrSegment::make_bytes(text.as_bytes())
        };
        let qr = QrCode::encode_segments(&[seg], QrCodeEcc::Medium).map_err(|_| TooLong)?;
        let size = usize::try_from(qr.size()).map_err(|_| TooLong)?;
        let mut dark = Vec::with_capacity(size * size);
        for y in 0..qr.size() {
            for x in 0..qr.size() {
                dark.push(qr.get_module(x, y));
            }
        }
        Ok(Self { size, dark })
    }

    /// Modules per side, 21 to 177.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Whether the module at column `x`, row `y` is dark; out of range is light.
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.dark[y * self.size + x]
    }

    /// An SVG with a `border` of light modules on each side (4 is the
    /// standard quiet zone). One path, crisp edges, no scripts.
    #[must_use]
    pub fn to_svg(&self, border: usize) -> String {
        let total = self.size + 2 * border;
        let mut s = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {total} {total}\" shape-rendering=\"crispEdges\">\
             <rect width=\"{total}\" height=\"{total}\" fill=\"#fff\"/><path fill=\"#000\" d=\""
        );
        for y in 0..self.size {
            for x in 0..self.size {
                if self.get(x, y) {
                    let _ = write!(s, "M{},{}h1v1h-1z", x + border, y + border);
                }
            }
        }
        s.push_str("\"/></svg>\n");
        s
    }

    /// The symbol as text, two rows per line with half blocks, light modules
    /// drawn as blocks — readable by a phone off a dark terminal.
    #[must_use]
    pub fn to_terminal(&self, border: usize) -> String {
        let total = self.size + 2 * border;
        let light = |x: usize, y: usize| x < border || y < border || !self.get(x - border, y - border);
        let mut s = String::new();
        for y in (0..total).step_by(2) {
            for x in 0..total {
                let top = light(x, y);
                let bottom = y + 1 >= total || light(x, y + 1);
                s.push(match (top, bottom) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    (false, false) => ' ',
                });
            }
            s.push('\n');
        }
        s
    }
}
