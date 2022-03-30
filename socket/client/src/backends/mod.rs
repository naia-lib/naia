cfg_if! {
    if #[cfg(all(target_arch = "wasm32", feature = "wbindgen"))] {
        mod wasm_bindgen;
        pub use self::wasm_bindgen::packet_sender::PacketSender;
        pub use self::wasm_bindgen::packet_receiver::PacketReceiverImpl;
        pub use self::wasm_bindgen::socket::Socket;
    }
    else if #[cfg(all(target_arch = "wasm32", feature = "mquad"))] {
        mod miniquad;
        pub use self::miniquad::packet_sender::PacketSender;
        pub use self::miniquad::packet_receiver::PacketReceiverImpl;
        pub use self::miniquad::socket::Socket;
    }
    else {
        mod native;
        pub use self::native::packet_sender::PacketSender;
        pub use self::native::packet_receiver::PacketReceiverImpl;
        pub use native::socket::Socket;
    }
}
