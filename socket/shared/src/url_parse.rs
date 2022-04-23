use std::net::SocketAddr;

use url::Url;

pub fn parse_server_url(server_url_str: &str) -> Url {
    let url = Url::parse(server_url_str).expect("server_url_str is not a valid URL!");
    if let Some(path_segments) = url.path_segments() {
        let path_segment_count = path_segments.count();
        if path_segment_count > 1 {
            log::error!("server_url_str must not include a path");
            panic!("");
        }
    }
    if url.query().is_some() {
        log::error!("server_url_str must not include a query string");
        panic!("");
    }
    if url.fragment().is_some() {
        log::error!("server_url_str must not include a fragment");
        panic!("");
    }

    url
}

cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))]
    {
        pub fn url_to_socket_addr(url: &Url) -> SocketAddr {
            const SOCKET_PARSE_FAIL_STR: &str = "could not get SocketAddr from input URL";

            match url.socket_addrs(|| match url.scheme() {
                "http" => Some(80),
                "https" => Some(443),
                _ => None,
            }) {
                Ok(addr_list) => {
                    if addr_list.is_empty() {
                        log::error!("{}", SOCKET_PARSE_FAIL_STR);
                        panic!("");
                    }

                    return *addr_list.first().expect(SOCKET_PARSE_FAIL_STR);
                }
                Err(err) => {
                    log::error!("URL -> SocketAddr parse fails with: {:?}", err);
                    panic!("");
                }
            }
        }
    } else {
        pub fn url_to_socket_addr(_url: &Url) -> SocketAddr {
            panic!("should not need this method for Wasm apps");
        }
    }
}
