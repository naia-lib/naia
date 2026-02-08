mod reject_reason;
pub use reject_reason::RejectReason;

cfg_if! {
    if #[cfg(feature = "advanced_handshake")] {
        pub mod advanced;
        pub use advanced::*;
    } else {
        pub mod simple;
        pub use simple::*;
    }
}
