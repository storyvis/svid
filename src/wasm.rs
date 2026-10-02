//! JS / TypeScript bindings via `wasm-bindgen`.
//!
//! All entry points marshal `i64` <-> JS `bigint` natively (no `as_f64`
//! precision loss). Fallible decoders return `Result<_, JsError>` so JS
//! receives a real thrown `Error`.

use wasm_bindgen::prelude::*;

use crate::SvidGenerator;
use crate::type_bits::{
    HUMAN_READABLE_LEN, SVID_EPOCH, SvidExt, decode_i64_base58, encode_svid, human_readable_to_id,
    human_readable_to_id_expecting, id_to_human_readable,
};

#[wasm_bindgen(typescript_custom_section)]
const TS_DECODED_SVID: &'static str = r#"
export interface DecodedSvid {
    timestamp: number;
    isClient: boolean;
    idType: number;
    random: number;
    unixTimestamp: bigint;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "DecodedSvid")]
    pub type DecodedSvid;
}

#[wasm_bindgen(js_name = generateSvid)]
pub fn generate_svid(id_type: u16) -> i64 {
    assert!(
        id_type <= 4095,
        "id_type {} exceeds 12-bit range (0..=4095)",
        id_type
    );
    SvidGenerator::generate(id_type, true)
}

#[wasm_bindgen(js_name = encodeSvid)]
pub fn encode_svid_js(timestamp: u32, is_client: bool, id_type: u16, random: u32) -> i64 {
    encode_svid(timestamp, is_client, id_type, random)
}

#[wasm_bindgen(js_name = decodeSvid)]
pub fn decode_svid(id: i64) -> DecodedSvid {
    let obj = js_sys::Object::new();
    let set = |k: &str, v: &JsValue| {
        js_sys::Reflect::set(&obj, &JsValue::from_str(k), v)
            .expect("Reflect::set on fresh Object cannot fail");
    };
    set("timestamp", &JsValue::from(id.timestamp_bits()));
    set("isClient", &JsValue::from_bool(id.is_client()));
    set("idType", &JsValue::from(id.tag()));
    set("random", &JsValue::from(id.random_bits()));
    set(
        "unixTimestamp",
        &js_sys::BigInt::from(id.unix_timestamp()).into(),
    );
    JsValue::from(obj).unchecked_into::<DecodedSvid>()
}

#[wasm_bindgen(js_name = encodeBase58)]
pub fn encode_base58(id: i64) -> String {
    bs58::encode(id.to_be_bytes()).into_string()
}

#[wasm_bindgen(js_name = decodeBase58)]
pub fn decode_base58(s: &str) -> Result<i64, JsError> {
    decode_i64_base58(s).map_err(|e| JsError::new(&e))
}

#[wasm_bindgen(js_name = encodeHumanReadable)]
pub fn encode_human_readable(id: i64) -> String {
    id_to_human_readable(id)
}

#[wasm_bindgen(js_name = decodeHumanReadable)]
pub fn decode_human_readable(s: &str) -> Result<i64, JsError> {
    human_readable_to_id(s).map_err(|e| JsError::new(&e))
}

#[wasm_bindgen(js_name = decodeHumanReadableExpecting)]
pub fn decode_human_readable_expecting(s: &str, expected_tag: u16) -> Result<i64, JsError> {
    human_readable_to_id_expecting(s, expected_tag).map_err(|e| JsError::new(&e))
}

#[wasm_bindgen(js_name = extractTag)]
pub fn extract_tag(id: i64) -> u16 {
    id.tag()
}

#[wasm_bindgen(js_name = extractTimestampBits)]
pub fn extract_timestamp_bits(id: i64) -> u32 {
    id.timestamp_bits()
}

#[wasm_bindgen(js_name = extractIsClient)]
pub fn extract_is_client(id: i64) -> bool {
    id.is_client()
}

#[wasm_bindgen(js_name = extractRandomBits)]
pub fn extract_random_bits(id: i64) -> u32 {
    id.random_bits()
}

#[wasm_bindgen(js_name = extractUnixTimestamp)]
pub fn extract_unix_timestamp(id: i64) -> i64 {
    id.unix_timestamp()
}

#[wasm_bindgen(js_name = svidEpoch)]
pub fn svid_epoch() -> i64 {
    SVID_EPOCH
}

#[wasm_bindgen(js_name = humanReadableLen)]
pub fn human_readable_len() -> u32 {
    HUMAN_READABLE_LEN as u32
}

/// UUID text avoids loss of precision in JavaScript numbers.
#[wasm_bindgen(js_name = generateSvid128)]
pub fn generate_svid128(id_type: u16) -> String {
    crate::Svid128::generate_with_source(id_type, true).to_string()
}

#[wasm_bindgen(js_name = normalizeSvid128)]
pub fn normalize_svid128(value: &str) -> Result<String, JsError> {
    value
        .parse::<crate::Svid128>()
        .map(|id| id.to_string())
        .map_err(|e| JsError::new(&e))
}

#[wasm_bindgen(js_name = extractSvid128Tag)]
pub fn extract_svid128_tag(value: &str) -> Result<u16, JsError> {
    value
        .parse::<crate::Svid128>()
        .map(|id| id.tag())
        .map_err(|e| JsError::new(&e))
}
