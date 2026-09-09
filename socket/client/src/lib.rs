//! # Naia Client Socket
//! A Socket abstraction over either a UDP socket on native Linux, or a
//! unreliable WebRTC datachannel on the browser

#![deny(unstable_features, unused_import_braces, unused_qualifications)]

extern crate log;

#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod wasm_utils;
    } else {
        // Only the wasm backends call this, but its tests are worth running on
        // the host: `cargo test` never targets wasm32, so a wasm-only module is
        // a module whose tests never execute.
        #[cfg(test)]
        mod wasm_utils;
    }
}

mod backends;
mod conditioned_packet_receiver;
mod error;
mod identity_receiver;
mod packet_receiver;
mod server_addr;

pub use naia_socket_shared as shared;

pub use backends::*;
pub use conditioned_packet_receiver::ConditionedPacketReceiver;
pub use error::NaiaClientSocketError;
pub use identity_receiver::IdentityReceiverResult;
pub use packet_receiver::PacketReceiver;
pub use server_addr::ServerAddr;

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", feature = "wbindgen", feature = "mquad"))]
    {
        // Use both protocols...
        compile_error!("Naia Client Socket on Wasm requires either the 'wbindgen' OR 'mquad' feature to be enabled, you must pick one.");
    }
    else if #[cfg(all(target_arch = "wasm32", not(feature = "wbindgen"), not(feature = "mquad")))]
    {
        // Use no protocols...
        compile_error!("Naia Client Socket on Wasm requires either the 'wbindgen' or 'mquad' feature to be enabled, you must pick one.");
    }
}

/// Host-executable oracle for the miniquad JavaScript bridge contract (F12).
///
/// The miniquad backend compiles only under
/// `cfg(all(target_arch = "wasm32", feature = "mquad"))`. There are contract
/// tests inside that module too, and they are the right tests -- but `cargo
/// test` never targets wasm32, so a test written there is a test that never
/// runs. The assertions below are the same contract, hoisted to the crate root
/// where the host toolchain does execute them. They supplement the wasm-gated
/// tests; they do not replace or weaken them.
///
/// Everything here reads the *shipped* artifacts through one crate-local path
/// each -- `include_str!` of the real `naia_socket.js` and the real
/// `shared.rs`. Nothing is transcribed, so there is no second copy to drift.
/// The header name is not spelled out either: it comes from
/// `naia_socket_shared`, the single authority, so renaming the constant without
/// editing the JavaScript reds this module rather than silently shipping two
/// header names.
#[cfg(test)]
mod miniquad_js_bridge_host_oracle {
    use naia_socket_shared::PROTOCOL_ID_HEADER;

    /// The JavaScript half of the bridge, exactly as it ships.
    const NAIA_SOCKET_JS: &str = include_str!("backends/miniquad/naia_socket.js");

    /// The Rust half. `extern "C"` declarations are checked by nobody: a
    /// mismatch between this and the JS import object is a runtime failure in a
    /// browser, not a compile error here, which is precisely why it is worth an
    /// oracle.
    const MINIQUAD_SHARED_RS: &str = include_str!("backends/miniquad/shared.rs");

    /// Returns the parameter names of the first parameter list following
    /// `after`. Works on both halves: JavaScript parameters are bare names and
    /// Rust parameters are `name: Type`, so taking the text before the first
    /// colon yields the name either way.
    fn parameter_names(source: &str, after: &str) -> Vec<String> {
        let start = source
            .find(after)
            .unwrap_or_else(|| panic!("could not find `{after}`"));
        let open = source[start..]
            .find('(')
            .unwrap_or_else(|| panic!("`{after}` has no parameter list"))
            + start;
        let close = source[open..]
            .find(')')
            .unwrap_or_else(|| panic!("`{after}` has an unterminated parameter list"))
            + open;

        source[open + 1..close]
            .split(',')
            .map(str::trim)
            .filter(|parameter| !parameter.is_empty())
            .map(|parameter| {
                parameter
                    .split(':')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
            .collect()
    }

    /// The exact byte sequence the shipped JavaScript must contain to set the
    /// fingerprint header. Built from the shared constant, never typed out.
    fn fingerprint_header_write() -> String {
        format!(r#"request.setRequestHeader("{PROTOCOL_ID_HEADER}""#)
    }

    /// The fingerprint must reach the request under the one canonical header
    /// name this workspace declares -- not a second spelling that happens to
    /// work today.
    #[test]
    fn the_js_bridge_uses_the_header_name_the_shared_crate_declares() {
        assert!(
            NAIA_SOCKET_JS.contains(&fingerprint_header_write()),
            "the miniquad JS bridge must set the `{PROTOCOL_ID_HEADER}` header",
        );
        assert_eq!(
            NAIA_SOCKET_JS.matches(PROTOCOL_ID_HEADER).count(),
            1,
            "the header name must appear exactly once in the JS bridge",
        );
    }

    /// Arity and order, across the FFI boundary in both directions.
    ///
    /// `extern "C" fn naia_connect` in `shared.rs`, the import-object binding,
    /// the JS `connect` definition and the call the binding forwards to must all
    /// name the same four parameters in the same order. Removing the fourth
    /// argument or moving it reds here, at the Rust/JS contract itself -- not
    /// later as an unrelated link failure or a silently misaligned argument in a
    /// browser.
    #[test]
    fn the_rust_declaration_and_the_js_bridge_agree_on_arity_and_order() {
        let rust_declaration = parameter_names(MINIQUAD_SHARED_RS, "pub fn naia_connect");
        let js_binding =
            parameter_names(NAIA_SOCKET_JS, "importObject.env.naia_connect = function");
        let js_forwarded_call = parameter_names(NAIA_SOCKET_JS, "naia_socket.connect(");
        let js_definition = parameter_names(NAIA_SOCKET_JS, "    connect: function (");

        let expected = [
            "server_socket_address",
            "rtc_path",
            "auth_str",
            "protocol_id",
        ];

        assert_eq!(
            rust_declaration, expected,
            "the Rust FFI declaration must take the fingerprint as its fourth argument",
        );
        assert_eq!(
            js_definition, expected,
            "the JS `connect` definition must match the Rust declaration",
        );
        // The binding and the forwarded call use the same names as each other
        // and have the same arity as the declaration; they are the wiring
        // between the two halves, so a reordering there is a real defect.
        assert_eq!(
            js_binding, js_forwarded_call,
            "the import-object binding must forward its arguments in order",
        );
        assert_eq!(
            js_binding.len(),
            expected.len(),
            "the import-object binding must take the same number of arguments as the Rust declaration",
        );
        assert_eq!(
            js_binding.last().map(String::as_str),
            Some("protocol_id"),
            "the fingerprint must be the fourth argument, not an optional trailing extra",
        );
    }

    /// The fourth argument must actually be *used*. A bridge that accepts the
    /// fingerprint and drops it would satisfy every arity check above while
    /// shipping an unfingerprinted request.
    #[test]
    fn the_js_bridge_threads_the_fourth_argument_into_the_request() {
        assert!(
            NAIA_SOCKET_JS.contains("naia_socket.get_js_object(protocol_id)"),
            "the fingerprint must be unwrapped through the same JsObject bridge as every other argument",
        );

        let unwrapped = NAIA_SOCKET_JS
            .find("naia_socket.get_js_object(protocol_id)")
            .expect("the fingerprint must be unwrapped");
        let written = NAIA_SOCKET_JS
            .find(&fingerprint_header_write())
            .expect("the fingerprint must be written to the request");
        assert!(
            unwrapped < written,
            "the fingerprint must be unwrapped before it is written to the request",
        );
    }

    /// A completed non-200 session POST is a signaling answer for the identity
    /// path, not a packet error. The JavaScript must hand its status and body
    /// to the dedicated `receive_auth_error` callback the Rust half exports;
    /// routing it through the generic `error` queue would strand the rejection
    /// as an unparseable diagnostic string, and waiting on a data channel the
    /// rejected handshake will never create hangs forever. Network-level
    /// failures (no status at all) stay on the generic path.
    #[test]
    fn non_200_signaling_answers_reach_the_identity_path_not_the_packet_queue() {
        // Both halves name the same two-parameter callback in the same order.
        assert_eq!(
            parameter_names(MINIQUAD_SHARED_RS, "pub extern \"C\" fn receive_auth_error"),
            ["status", "body"],
            "the Rust half must export receive_auth_error(status, body)",
        );
        assert!(
            NAIA_SOCKET_JS.contains("wasm_exports.receive_auth_error("),
            "the JS half must forward non-200 answers to receive_auth_error",
        );

        // The old ad-hoc routing -- stringifying the status into the generic
        // packet error queue -- must be gone from the completed-request path.
        assert!(
            !NAIA_SOCKET_JS.contains("{ response_status: request.status }"),
            "signaling status must not be stringified into the packet error queue",
        );

        // ... while the status-less network failure path keeps its generic
        // routing: it has no status to report.
        assert!(
            NAIA_SOCKET_JS.contains("request.onerror = function(err)"),
            "network-level POST failures must stay on the generic error path",
        );

        // The Rust half holds one bounded slot for the outstanding answer.
        assert!(
            MINIQUAD_SHARED_RS.contains("AUTH_ERROR_CELL"),
            "the Rust half must keep a dedicated bounded auth-error cell",
        );
    }

    /// The callback arguments are live values in the right positions: status
    /// first, body second. Swapping them would report the body as a status and
    /// decode the status as a reason; substituting a constant for either would
    /// blind the identity path to what the server actually answered. Both
    /// defects keep the callback name intact, so the routing oracle above
    /// stays green while the refusal itself corrupts -- hence this one reads
    /// the call's arguments, not just its name.
    #[test]
    fn the_js_callback_receives_the_live_status_then_the_live_body() {
        let call_start = NAIA_SOCKET_JS
            .find("wasm_exports.receive_auth_error(")
            .expect("the JS half must forward non-200 answers to receive_auth_error");
        let call = &NAIA_SOCKET_JS[call_start..];
        let call_end = call
            .find(");")
            .expect("the receive_auth_error call must be closed");
        let call = &call[..call_end];

        // Both arguments must be live request values, not constants: a
        // hardcoded status or body would erase the server's actual answer.
        let status = call
            .find("request.status")
            .expect("the first callback argument must be the live request status");
        let body = call
            .find("responseText")
            .expect("the second callback argument must be the live response body");

        // Order across the FFI boundary: status first, body second. The Rust
        // half declares `receive_auth_error(status, body)`, so a swap here
        // would silently exchange them.
        assert!(
            status < body,
            "the status must precede the body in the receive_auth_error call",
        );

        // The status crosses as a string through the same JsObject bridge as
        // every other argument -- a raw number would mistype at the boundary.
        assert!(
            call.contains("String(request.status)"),
            "the status must be stringified through the JsObject bridge",
        );
    }

    /// Framework-last, and unconditional.
    ///
    /// The caller's `Authorization` header goes on first and only if there is
    /// one; naia's fingerprint goes on afterwards and always. Last-wins on
    /// duplicate header names is what makes "the caller cannot replace it"
    /// structural rather than advisory, and being outside the credential's
    /// `if` block is what makes "the caller cannot omit it" true even for a
    /// connection that carries no credential at all.
    #[test]
    fn the_fingerprint_is_written_after_the_credential_and_unconditionally() {
        let credential_branch = NAIA_SOCKET_JS
            .find("if (auth_string.length > 0)")
            .expect("the credential must still be conditional");
        let credential_write = NAIA_SOCKET_JS
            .find(r#"request.setRequestHeader("Authorization""#)
            .expect("the caller-supplied credential header must still be written");
        let fingerprint_write = NAIA_SOCKET_JS
            .find(&fingerprint_header_write())
            .expect("the fingerprint header must be written");

        assert!(
            credential_branch < credential_write,
            "the credential write must sit inside its own conditional",
        );
        assert!(
            credential_write < fingerprint_write,
            "naia must stamp the fingerprint after the caller's headers, so a caller cannot replace it",
        );

        // The credential's `if` block must be closed before the fingerprint is
        // written, or the fingerprint would ride along with the credential and
        // a credential-less connection would go out unstamped.
        let branch_close = NAIA_SOCKET_JS[credential_write..]
            .find('}')
            .expect("the credential's conditional must be closed")
            + credential_write;
        assert!(
            branch_close < fingerprint_write,
            "the fingerprint must be written outside the credential's conditional",
        );
    }
}
