//! Formatting and parsing helpers for 128-bit entity registries.
//! UUID text is shared by JSON, URLs, PostgreSQL, and CRDT documents.
use crate::Svid128;
pub const HUMAN_READABLE_LEN: usize = Svid128::TEXT_LEN;
pub fn id_to_human_readable(id: Svid128) -> String {
    id.to_string()
}
pub fn encode_str_into(id: Svid128, buf: &mut [u8; HUMAN_READABLE_LEN]) -> &str {
    id.encode_into(buf)
}
pub fn human_readable_to_id(value: &str) -> Result<Svid128, String> {
    value.parse()
}
pub fn human_readable_to_id_expecting(value: &str, tag: u16) -> Result<Svid128, String> {
    let id: Svid128 = value.parse()?;
    if id.tag() != tag {
        return Err(format!(
            "Invalid SVID tag: expected {tag}, got {}",
            id.tag()
        ));
    }
    Ok(id)
}
