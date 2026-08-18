//! Shared image identity and hashing primitives.
//!
//! These are the byte-level building blocks used by every "generate an image
//! and post it to the group" workflow (huabaosi AI image generation, the
//! Xiaoman daily case report, and the Erhua morning brief). They were
//! previously duplicated across `image_generation.rs` and `operations.rs`;
//! this module is the single home so all three workflows share one
//! implementation.
//!
//! These are pure functions (no I/O, no network, no DB). File reading and
//! workflow-specific validation stay with the callers.

use md5::Md5;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Compute the canonical `sha256:<lowerhex>` content hash for raw bytes.
pub fn content_hash_bytes(value: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(value))
}

/// Compute the lowercase hex MD5 for raw bytes.
pub fn md5_hex_bytes(value: &[u8]) -> String {
    format!("{:x}", Md5::digest(value))
}

/// Hash a list of string parts joined by NUL separators into `sha256:<hex>`.
///
/// Used to build stable identity keys (deterministic UUIDs, idempotency keys,
/// group hashes) from several fields without ambiguity between boundaries.
pub fn null_separated_digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

/// Derive a deterministic UUID (version/variant bits set) from string parts.
///
/// The same parts always produce the same UUID, so a workflow can regenerate
/// the identical artifact/work-item id for the same logical image instead of
/// creating duplicates.
pub fn deterministic_uuid_from_parts(parts: &[&str]) -> Uuid {
    let digest = null_separated_digest(parts);
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest.as_str());
    let mut bytes = [0_u8; 16];
    for (index, slot) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        *slot = u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap_or_default();
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Identity of a JPEG image held in memory.
///
/// `bytes` holds the raw JPEG payload; callers are responsible for zeroizing
/// it on drop when it is sensitive (the workflows wrap it in their own
/// zeroizing types).
#[derive(Debug, Clone)]
pub struct ImageIdentity {
    pub bytes: Vec<u8>,
    pub content_hash: String,
    pub file_md5: String,
    pub byte_size: usize,
    pub width: u32,
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_and_md5_match_known_vectors() {
        assert_eq!(
            content_hash_bytes(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(md5_hex_bytes(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn null_separated_digest_is_boundary_safe() {
        // "ab","c" must differ from "a","bc".
        assert_ne!(
            null_separated_digest(&["ab", "c"]),
            null_separated_digest(&["a", "bc"])
        );
    }

    #[test]
    fn deterministic_uuid_is_stable_and_sets_bits() {
        let first = deterministic_uuid_from_parts(&["seed", "1"]);
        let second = deterministic_uuid_from_parts(&["seed", "1"]);
        let other = deterministic_uuid_from_parts(&["seed", "2"]);
        assert_eq!(first, second);
        assert_ne!(first, other);
        let bytes = first.as_bytes();
        assert_eq!(bytes[6] >> 4, 0x8); // version 8
        assert_eq!(bytes[8] >> 6, 0x2); // variant RFC 4122
    }
}
