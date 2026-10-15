//! AWS event-stream framing (lengths and both CRC32 checksums are validated).
use anyhow::{bail, Context, Result};
use serde_json::Value;

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
fn number(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}

pub(super) fn next(pending: &mut Vec<u8>) -> Result<Option<Value>> {
    if pending.len() < 12 {
        return Ok(None);
    }
    let total = number(&pending[..4]) as usize;
    let headers_len = number(&pending[4..8]) as usize;
    if !(16..=16 * 1024 * 1024).contains(&total) || headers_len > total - 16 {
        bail!("Invalid AWS event-stream frame length");
    }
    if crc32(&pending[..8]) != number(&pending[8..12]) {
        bail!("AWS event-stream prelude checksum mismatch");
    }
    if pending.len() < total {
        return Ok(None);
    }
    if crc32(&pending[..total - 4]) != number(&pending[total - 4..total]) {
        bail!("AWS event-stream message checksum mismatch");
    }
    let headers = &pending[12..12 + headers_len];
    let mut offset = 0;
    let mut event_type = None;
    let mut message_type = None;
    while offset < headers.len() {
        let len = headers[offset] as usize;
        offset += 1;
        let name = std::str::from_utf8(
            headers
                .get(offset..offset + len)
                .context("Truncated AWS header name")?,
        )?;
        offset += len;
        let kind = *headers.get(offset).context("Truncated AWS header type")?;
        offset += 1;
        if kind != 7 {
            bail!("Unsupported AWS event-stream header type {kind}");
        }
        let len_bytes = headers
            .get(offset..offset + 2)
            .context("Truncated AWS header length")?;
        let len = u16::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
        offset += 2;
        let value = std::str::from_utf8(
            headers
                .get(offset..offset + len)
                .context("Truncated AWS header value")?,
        )?;
        offset += len;
        if name == ":event-type" || name == ":exception-type" {
            event_type = Some(value.to_string());
        }
        if name == ":message-type" {
            message_type = Some(value.to_string());
        }
    }
    let payload: Value = serde_json::from_slice(&pending[12 + headers_len..total - 4])?;
    if message_type.as_deref() == Some("exception") || message_type.as_deref() == Some("error") {
        bail!(
            "Bedrock stream exception {}: {}",
            event_type.unwrap_or_default(),
            payload["message"]
        );
    }
    let event_type = event_type.context("AWS event has no event type")?;
    pending.drain(..total);
    Ok(Some(
        serde_json::json!({"type":event_type,"payload":payload}),
    ))
}

#[cfg(test)]
pub(super) fn frame(kind: &str, value: Value) -> Vec<u8> {
    let mut headers = Vec::new();
    for (key, value) in [(":message-type", "event"), (":event-type", kind)] {
        headers.push(key.len() as u8);
        headers.extend_from_slice(key.as_bytes());
        headers.push(7);
        headers.extend_from_slice(&(value.len() as u16).to_be_bytes());
        headers.extend_from_slice(value.as_bytes());
    }
    let body = value.to_string();
    let total = 16 + headers.len() + body.len();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(total as u32).to_be_bytes());
    bytes.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&crc32(&bytes).to_be_bytes());
    bytes.extend(headers);
    bytes.extend_from_slice(body.as_bytes());
    bytes.extend_from_slice(&crc32(&bytes).to_be_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_validates_crc_and_fragment_boundaries() {
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
        let encoded = frame("messageStop", serde_json::json!({"stopReason":"end_turn"}));
        let mut pending = encoded[..13].to_vec();
        assert!(next(&mut pending).unwrap().is_none());
        pending.extend_from_slice(&encoded[13..]);
        assert_eq!(
            next(&mut pending).unwrap().unwrap()["payload"]["stopReason"],
            "end_turn"
        );
        assert!(pending.is_empty());
        let mut corrupt = encoded;
        corrupt[15] ^= 1;
        assert!(next(&mut corrupt).is_err());
    }
}
