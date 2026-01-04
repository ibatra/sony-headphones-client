//! Sony headphones protocol codec
//! Handles message framing, escaping, and checksum calculation

use super::constants::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("Message exceeds maximum size of {MAX_BLUETOOTH_MESSAGE_SIZE} bytes")]
    MessageTooLarge,

    #[error("Invalid message: smaller than minimum size")]
    MessageTooSmall,

    #[error("No data left for escaped byte")]
    IncompleteEscapeSequence,

    #[error("Unexpected escaped byte: {0}")]
    UnexpectedEscapedByte(u8),

    #[error("Invalid checksum: expected {expected}, got {actual}")]
    InvalidChecksum { expected: u8, actual: u8 },

    #[error("Missing start marker")]
    MissingStartMarker,

    #[error("Missing end marker")]
    MissingEndMarker,
}

/// Parsed message from Bluetooth
#[derive(Debug, Clone)]
pub struct Message {
    pub data_type: DataType,
    pub seq_number: u8,
    pub payload: Vec<u8>,
}

/// Escape special bytes in the data
/// 60 -> 61, 44
/// 61 -> 61, 45
/// 62 -> 61, 46
pub fn escape_specials(src: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(src.len());

    for &byte in src {
        match byte {
            60 => {
                result.push(ESCAPED_BYTE_SENTRY);
                result.push(ESCAPED_60);
            }
            61 => {
                result.push(ESCAPED_BYTE_SENTRY);
                result.push(ESCAPED_61);
            }
            62 => {
                result.push(ESCAPED_BYTE_SENTRY);
                result.push(ESCAPED_62);
            }
            _ => {
                result.push(byte);
            }
        }
    }

    result
}

/// Unescape special bytes in the data
pub fn unescape_specials(src: &[u8]) -> Result<Vec<u8>, CodecError> {
    let mut result = Vec::with_capacity(src.len());
    let mut i = 0;

    while i < src.len() {
        let byte = src[i];

        if byte == ESCAPED_BYTE_SENTRY {
            if i + 1 >= src.len() {
                return Err(CodecError::IncompleteEscapeSequence);
            }

            i += 1;
            let escaped_byte = src[i];

            match escaped_byte {
                ESCAPED_60 => result.push(60),
                ESCAPED_61 => result.push(61),
                ESCAPED_62 => result.push(62),
                _ => return Err(CodecError::UnexpectedEscapedByte(escaped_byte)),
            }
        } else {
            result.push(byte);
        }

        i += 1;
    }

    Ok(result)
}

/// Calculate checksum (simple sum of all bytes)
pub fn calculate_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// Convert u32 to big-endian bytes
pub fn u32_to_be_bytes(num: u32) -> [u8; 4] {
    num.to_be_bytes()
}

/// Convert big-endian bytes to u32
pub fn be_bytes_to_u32(bytes: &[u8]) -> u32 {
    if bytes.len() < 4 {
        return 0;
    }
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Package data for Bluetooth transmission
/// Format: <START_MARKER>ESCAPE(<DATA_TYPE><SEQ_NUM><SIZE_BE_4><DATA><CHECKSUM>)<END_MARKER>
pub fn package_for_bluetooth(
    payload: &[u8],
    data_type: DataType,
    seq_number: u8,
) -> Result<Vec<u8>, CodecError> {
    // Build the inner data (before escaping)
    let mut inner = Vec::with_capacity(payload.len() + 7);

    // Data type
    inner.push(data_type as u8);

    // Sequence number
    inner.push(seq_number);

    // Size (4 bytes, big-endian)
    let size_bytes = u32_to_be_bytes(payload.len() as u32);
    inner.extend_from_slice(&size_bytes);

    // Payload
    inner.extend_from_slice(payload);

    // Calculate and append checksum
    let checksum = calculate_checksum(&inner);
    inner.push(checksum);

    // Escape special bytes
    let escaped = escape_specials(&inner);

    // Build final message with markers
    let mut result = Vec::with_capacity(escaped.len() + 2);
    result.push(START_MARKER);
    result.extend_from_slice(&escaped);
    result.push(END_MARKER);

    // Check size limit
    if result.len() > MAX_BLUETOOTH_MESSAGE_SIZE {
        return Err(CodecError::MessageTooLarge);
    }

    Ok(result)
}

/// Unpack a Bluetooth message
/// Expects the raw message including start/end markers
pub fn unpack_bluetooth_message(raw: &[u8]) -> Result<Message, CodecError> {
    if raw.len() < 2 {
        return Err(CodecError::MessageTooSmall);
    }

    // Verify markers
    if raw[0] != START_MARKER {
        return Err(CodecError::MissingStartMarker);
    }
    if raw[raw.len() - 1] != END_MARKER {
        return Err(CodecError::MissingEndMarker);
    }

    // Extract inner content (without markers)
    let inner = &raw[1..raw.len() - 1];

    // Unescape
    let unescaped = unescape_specials(inner)?;

    // Minimum size: data_type(1) + seq(1) + size(4) + checksum(1) = 7
    if unescaped.len() < 7 {
        return Err(CodecError::MessageTooSmall);
    }

    // Verify checksum
    let checksum_idx = unescaped.len() - 1;
    let expected_checksum = unescaped[checksum_idx];
    let actual_checksum = calculate_checksum(&unescaped[..checksum_idx]);

    if expected_checksum != actual_checksum {
        // Log warning but continue - some devices may use different checksum
        tracing::warn!(
            "Checksum mismatch: expected {}, got {}. Raw data: {:02X?}",
            expected_checksum,
            actual_checksum,
            &unescaped[..std::cmp::min(20, unescaped.len())]
        );
        // Continue parsing anyway - the data might still be valid
    }

    // Parse message
    let data_type = DataType::from(unescaped[0]);
    let seq_number = unescaped[1];
    let _size = be_bytes_to_u32(&unescaped[2..6]);
    let payload = unescaped[6..checksum_idx].to_vec();

    Ok(Message {
        data_type,
        seq_number,
        payload,
    })
}

/// Extract a complete message from a stream buffer
/// Returns (message_bytes, remaining_bytes) if a complete message is found
pub fn extract_message(buffer: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    // Find start marker
    let start_pos = buffer.iter().position(|&b| b == START_MARKER)?;

    // Find end marker after start
    let search_start = start_pos + 1;
    let end_pos = buffer[search_start..]
        .iter()
        .position(|&b| b == END_MARKER)
        .map(|p| p + search_start)?;

    // Extract message including markers
    let message = buffer[start_pos..=end_pos].to_vec();
    let remaining = buffer[end_pos + 1..].to_vec();

    Some((message, remaining))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_unescape_roundtrip() {
        let original = vec![0, 59, 60, 61, 62, 63, 100];
        let escaped = escape_specials(&original);
        let unescaped = unescape_specials(&escaped).unwrap();
        assert_eq!(original, unescaped);
    }

    #[test]
    fn test_escape_specials() {
        let input = vec![60, 61, 62];
        let escaped = escape_specials(&input);
        assert_eq!(escaped, vec![61, 44, 61, 45, 61, 46]);
    }

    #[test]
    fn test_checksum() {
        let data = vec![1, 2, 3, 4, 5];
        assert_eq!(calculate_checksum(&data), 15);
    }

    #[test]
    fn test_package_and_unpack() {
        let payload = vec![104, 2, 1, 0, 2, 1, 0, 5];
        let packaged = package_for_bluetooth(&payload, DataType::DataMdr, 1).unwrap();
        let unpacked = unpack_bluetooth_message(&packaged).unwrap();

        assert_eq!(unpacked.data_type, DataType::DataMdr);
        assert_eq!(unpacked.seq_number, 1);
        assert_eq!(unpacked.payload, payload);
    }
}
