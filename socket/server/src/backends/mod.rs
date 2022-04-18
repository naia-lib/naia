cfg_if! {
    if #[cfg(feature = "use-udp")] {
        mod udp;
        pub use self::udp::socket::Socket;
    }
    else if #[cfg(feature = "use-webrtc")] {
        mod webrtc;
        pub use self::webrtc::socket::Socket;
    }
    else {
    }
}
