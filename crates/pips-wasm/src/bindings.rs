//! JavaScript bindings: [`crate::api`] with camelCase names.
//!
//! Data crosses as plain objects; byte fields arrive as `Uint8Array`.
//! Errors are thrown as `{ code, message }` objects, so the app can switch
//! on `code` and translate. Every `Result` below fails that way, with the
//! codes listed in [`crate::api`].
#![allow(clippy::missing_errors_doc)]
// Signatures are shaped by the JS boundary: wasm-bindgen passes owned
// values, and return values are consumed by JavaScript, not Rust.
#![allow(clippy::needless_pass_by_value, clippy::must_use_candidate)]

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::api::{self, ApiError, CardData};

type Js<T> = Result<T, JsValue>;

fn js<T: Serialize>(value: &T) -> Js<JsValue> {
    value.serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true)).map_err(Into::into)
}

#[allow(clippy::needless_pass_by_value)] // used with map_err
fn thrown(e: ApiError) -> JsValue {
    js(&e).unwrap_or_else(|_| JsValue::from_str(&e.message))
}

fn card(value: JsValue) -> Js<CardData> {
    serde_wasm_bindgen::from_value(value).map_err(|e| thrown(ApiError::new("invalid_card", e)))
}

/// Every built-in palette.
#[wasm_bindgen]
pub fn palettes() -> Js<JsValue> {
    js(&api::palettes())
}

/// Reads a `.hex` or `.gpl` palette file.
#[wasm_bindgen(js_name = parsePalette)]
pub fn parse_palette(bytes: &[u8]) -> Js<JsValue> {
    js(&api::parse_palette(bytes).map_err(thrown)?)
}

/// Bytes the card takes when shared; compare with 600 for one code.
#[wasm_bindgen(js_name = shareSize)]
pub fn share_size(card_data: JsValue) -> Js<usize> {
    api::share_size(&card(card_data)?).map_err(thrown)
}

/// One frame as `{ width, height, rgba }`.
#[wasm_bindgen]
pub fn render(card_data: JsValue, frame: u8) -> Js<JsValue> {
    js(&api::render(&card(card_data)?, frame).map_err(thrown)?)
}

/// Verifies and unpacks an envelope.
#[wasm_bindgen(js_name = openEnvelope)]
pub fn open_envelope(bytes: &[u8]) -> Js<JsValue> {
    js(&api::open_envelope(bytes).map_err(thrown)?)
}

/// Checks and unpacks stored card payload bytes.
#[wasm_bindgen(js_name = inspectCard)]
pub fn inspect_card(payload: &[u8]) -> Js<JsValue> {
    js(&api::inspect_card(payload).map_err(thrown)?)
}

/// Turns a PNG or Aseprite file into `{ card, sourceWidth, sourceHeight,
/// sourceFrames, scale, colors, changedPixels }`. `options` may be omitted:
/// `{ keepScale, firstFrameOnly, palette }`.
#[wasm_bindgen(js_name = importPicture)]
pub fn import_picture(bytes: &[u8], options: JsValue) -> Js<JsValue> {
    let o = if options.is_undefined() || options.is_null() {
        api::ImportOptions::default()
    } else {
        serde_wasm_bindgen::from_value(options).map_err(|e| thrown(ApiError::new("bad_options", e)))?
    };
    js(&api::import_picture(bytes, o).map_err(thrown)?)
}

/// Whose backup a file is, `{ hex, short }`, without any key.
#[wasm_bindgen(js_name = backupOwner)]
pub fn backup_owner(bytes: &[u8]) -> Js<JsValue> {
    js(&api::backup_owner(bytes).map_err(thrown)?)
}

/// Recognises scanned text.
#[wasm_bindgen]
pub fn classify(text: &str) -> Js<JsValue> {
    js(&api::classify(text))
}

/// A link that opens the app on an envelope.
#[wasm_bindgen(js_name = cardLink)]
pub fn card_link(base: &str, envelope: &[u8]) -> Js<String> {
    api::card_link(base, envelope).map_err(thrown)
}

/// The envelope inside a card link.
#[wasm_bindgen(js_name = parseLink)]
pub fn parse_link(url: &str) -> Js<Vec<u8>> {
    api::parse_link(url).map_err(thrown)
}

/// One code (`{ kind: "single", text }`) or a carousel (`{ kind: "carousel", fragments }`).
#[wasm_bindgen(js_name = qrPlan)]
pub fn qr_plan(bytes: &[u8]) -> Js<JsValue> {
    js(&api::qr_plan(bytes).map_err(thrown)?)
}

/// `{ size, modules }` for drawing on a canvas.
#[wasm_bindgen(js_name = qrMatrix)]
pub fn qr_matrix(text: &str) -> Js<JsValue> {
    js(&api::qr_matrix(text).map_err(thrown)?)
}

/// An SVG QR code, for printing.
#[wasm_bindgen(js_name = qrSvg)]
pub fn qr_svg(text: &str) -> Js<String> {
    api::qr_svg(text).map_err(thrown)
}

/// The user's key.
#[wasm_bindgen]
pub struct Identity(api::Identity);

#[wasm_bindgen]
impl Identity {
    /// From the stored seed or 32 fresh random bytes.
    #[wasm_bindgen(js_name = fromSeed)]
    pub fn from_seed(seed: &[u8]) -> Js<Identity> {
        api::Identity::from_seed(seed).map(Self).map_err(thrown)
    }
    /// From the 24 recovery words.
    #[wasm_bindgen(js_name = fromWords)]
    pub fn from_words(words: &str) -> Js<Identity> {
        api::Identity::from_words(words).map(Self).map_err(thrown)
    }
    /// From a scanned paper key.
    #[wasm_bindgen(js_name = fromPaper)]
    pub fn from_paper(text: &str) -> Js<Identity> {
        api::Identity::from_paper(text).map(Self).map_err(thrown)
    }
    /// The seed to store.
    pub fn seed(&self) -> Vec<u8> {
        self.0.seed()
    }
    /// The 24 recovery words.
    pub fn words(&self) -> String {
        self.0.words()
    }
    /// The paper-key text.
    pub fn paper(&self) -> String {
        self.0.paper()
    }
    /// `{ hex, short }`.
    #[wasm_bindgen(js_name = publicKey)]
    pub fn public_key(&self) -> Js<JsValue> {
        js(&self.0.public_key())
    }
    /// Encrypts `{ created, profile, cards }` to this identity.
    #[wasm_bindgen(js_name = sealBackup)]
    pub fn seal_backup(&self, data: JsValue, nonce: &[u8]) -> Js<Vec<u8>> {
        let d = serde_wasm_bindgen::from_value(data).map_err(|e| thrown(ApiError::new("invalid_backup", e)))?;
        self.0.seal_backup(&d, nonce).map_err(thrown)
    }
    /// Decrypts and checks a backup of this identity.
    #[wasm_bindgen(js_name = openBackup)]
    pub fn open_backup(&self, bytes: &[u8]) -> Js<JsValue> {
        js(&self.0.open_backup(bytes).map_err(thrown)?)
    }
    /// Signs an editor card; returns its payload.
    #[wasm_bindgen(js_name = signCard)]
    pub fn sign_card(&self, card_data: JsValue) -> Js<Vec<u8>> {
        self.0.sign_card(&card(card_data)?).map_err(thrown)
    }
    /// Base45 text of this person's recipient code.
    #[wasm_bindgen(js_name = recipientCode)]
    pub fn recipient_code(&self, nick: &str) -> Js<String> {
        self.0.recipient_code(nick).map_err(thrown)
    }
    /// Issues copy `edition` of `of` to a scanned recipient code.
    pub fn issue(&self, payload: &[u8], edition: u16, of: u16, recipient: &[u8], day: u32) -> Js<Vec<u8>> {
        self.0.issue(payload, edition, of, recipient, day).map_err(thrown)
    }
    /// Envelope for an own card, with an optional certificate.
    #[wasm_bindgen(js_name = sealAsAuthor)]
    pub fn seal_as_author(&self, payload: &[u8], certificate: Option<Vec<u8>>) -> Js<Vec<u8>> {
        self.0.seal_as_author(payload, certificate.as_deref()).map_err(thrown)
    }
    /// Envelope for passing on someone else's card.
    #[wasm_bindgen(js_name = sealAsSender)]
    pub fn seal_as_sender(&self, payload: &[u8], nick: &str) -> Js<Vec<u8>> {
        self.0.seal_as_sender(payload, nick).map_err(thrown)
    }
}

/// A loop of codes for a large message.
#[wasm_bindgen]
pub struct Carousel(api::Carousel);

#[wasm_bindgen]
impl Carousel {
    /// A carousel for `bytes`.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Js<Carousel> {
        api::Carousel::new(bytes).map(Self).map_err(thrown)
    }
    /// Fewest frames that carry the message.
    pub fn fragments(&self) -> usize {
        self.0.fragments()
    }
    /// Base45 text of frame `seq`.
    #[wasm_bindgen(js_name = frameText)]
    pub fn frame_text(&self, seq: u32) -> String {
        self.0.frame_text(seq)
    }
}

/// Joins scanned carousel frames.
#[wasm_bindgen]
pub struct Joiner(api::Joiner);

#[wasm_bindgen]
impl Joiner {
    /// An empty joiner.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Joiner {
        Self(api::Joiner::new())
    }
    /// Adds a frame; `{ kind: "partial", have, need }` or `{ kind: "done", bytes }`.
    pub fn push(&mut self, frame: &[u8]) -> Js<JsValue> {
        js(&self.0.push(frame).map_err(thrown)?)
    }
}
