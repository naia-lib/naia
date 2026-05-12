mod reject_reason;
pub use reject_reason::RejectReason;

cfg_if! {
    if #[cfg(feature = "advanced_handshake")] {
        pub mod advanced;
        pub use advanced::*;
    } else {
        /// Standard handshake protocol for identifying clients and exchanging timing information.
        pub mod simple;
        pub use simple::*;
    }
}
