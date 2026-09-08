//! The wire name and width of the protocol-fingerprint auth header.
//!
//! This lives here, in `naia-socket-shared`, because it is the one crate that
//! sits below every consumer of the value: `naia-client-socket` and
//! `naia-server-socket` depend on it, and so does `naia-shared` (and through it
//! `naia-client` and `naia-server`). The fingerprint *type* — `ProtocolId` —
//! cannot come down here, since computing it needs the channel/message/component
//! registries that only `naia-shared` has. So the type stays up there and the
//! socket layer handles the value as fixed-width hex text.
//!
//! What must not happen is four copies of the header name: the client backend
//! that sets it and the session server that reads it would then be able to
//! drift apart silently, and the failure mode of that drift is every client
//! being refused, or — worse, if a comparison is skipped rather than failed —
//! nobody being checked at all. One constant, one compile-time authority.

/// Name of the HTTP header carrying the client's protocol fingerprint.
///
/// Lowercase: HTTP header names are case-insensitive, and the session server's
/// request scan lowercases each line before matching. The `x-naia-` prefix
/// keeps it clear of any header a consumer sets for its own purposes.
pub const PROTOCOL_ID_HEADER: &str = "x-naia-protocol-id";

/// Exact length of the header's value: a 128-bit fingerprint as 32 lowercase
/// hex digits, no `0x` prefix, no separators.
///
/// Fixed width is what lets a reader reject a malformed value without parsing
/// it, and it is why "absent", "too short", "too long", and "wrong" all cost
/// the same and are indistinguishable from outside.
pub const PROTOCOL_ID_HEADER_VALUE_LEN: usize = 32;

/// Appends the protocol-fingerprint header to a caller-supplied auth header
/// list, and returns the list the socket should actually send.
///
/// Naia stamps the fingerprint itself, after everything the consumer asked for:
///
/// - a caller cannot **omit** it, because every `connect` entry point routes
///   through here and none of them takes a "skip this" option;
/// - a caller cannot **replace** it, because any entry the caller supplied
///   under this name (in any casing) is dropped first;
/// - a caller cannot **strip** it, because the caller's list is consumed here
///   and never seen again.
///
/// It goes last so that even a downstream that resolves duplicate header names
/// as last-wins resolves to naia's value.
///
/// This is not authentication. The fingerprint is public compatibility
/// metadata: knowing it grants nothing, and a peer that supplies the right one
/// still has to authenticate normally.
pub fn stamp_protocol_id_header(
    auth_headers_opt: Option<Vec<(String, String)>>,
    protocol_id: &str,
) -> Vec<(String, String)> {
    let mut headers = auth_headers_opt.unwrap_or_default();
    headers.retain(|(name, _)| !name.eq_ignore_ascii_case(PROTOCOL_ID_HEADER));
    headers.push((PROTOCOL_ID_HEADER.to_string(), protocol_id.to_string()));
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamping_appends_the_fingerprint_last() {
        let headers = stamp_protocol_id_header(
            Some(vec![("X-Consumer".to_string(), "value".to_string())]),
            "0123456789abcdef0123456789abcdef",
        );
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0, "X-Consumer");
        assert_eq!(
            headers[1],
            (
                PROTOCOL_ID_HEADER.to_string(),
                "0123456789abcdef0123456789abcdef".to_string()
            )
        );
    }

    #[test]
    fn a_caller_cannot_replace_the_fingerprint() {
        // Including by casing the name differently, which HTTP would treat as
        // the same header.
        let headers = stamp_protocol_id_header(
            Some(vec![
                ("X-Naia-Protocol-Id".to_string(), "attacker".to_string()),
                ("x-naia-protocol-id".to_string(), "attacker".to_string()),
            ]),
            "0123456789abcdef0123456789abcdef",
        );
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].1, "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn a_caller_cannot_omit_the_fingerprint() {
        let headers = stamp_protocol_id_header(None, "0123456789abcdef0123456789abcdef");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, PROTOCOL_ID_HEADER);
    }

    #[test]
    fn the_header_name_is_lowercase_and_naia_scoped() {
        assert_eq!(PROTOCOL_ID_HEADER, PROTOCOL_ID_HEADER.to_lowercase());
        assert!(PROTOCOL_ID_HEADER.starts_with("x-naia-"));
    }
}
