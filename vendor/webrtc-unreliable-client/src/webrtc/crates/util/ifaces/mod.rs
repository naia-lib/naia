pub(crate) mod ffi;
pub(crate) use ffi::ifaces;

#[derive(Debug, Clone)]
pub(crate) struct Interface {
    pub(crate) name: String,
    pub(crate) addr: Option<::std::net::SocketAddr>,
    pub(crate) mask: Option<::std::net::SocketAddr>,
}
