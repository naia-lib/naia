use std::net::SocketAddr;

// Minimal URL parser for naia-socket-shared — replaces the `url` crate entirely.
//
// Why: `url` → `idna` → `icu_normalizer_data + icu_properties_data` pulls ~600 KB
// of ICU Unicode lookup tables into the wasm binary.  Game servers use ASCII
// hostnames (IPs or simple domain names), so full IDNA/Unicode URL parsing is
// dead weight on every platform.
//
// What we actually need from a server URL (`ws://host:port` / `http://host:port`):
//   – Display (to build the session URL string from a base + endpoint path)
//   – path_segments / query / fragment guards in `parse_server_url`
//   – host + port for native DNS resolution in `url_to_socket_addr`
//
// All of that is achievable with simple ASCII string parsing.

#[derive(Debug, Clone)]
pub struct Url {
    original: String,
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.original)?;
        // Normalize a bare-authority URL ("http://host:port") to a single "/"
        // path, matching the `url` crate this parser replaced
        // (`Url::parse("http://h:p").to_string()` == "http://h:p/").
        //
        // Display's documented job is to build the signaling URL by
        // concatenating a base with an endpoint path — both the wasm_bindgen
        // (`format!("{}{}", base, rtc_endpoint_path)`) and miniquad
        // (`address + rtc_path` in naia_socket.js) backends join with NO
        // separator and rely on the base ending in "/". Without this, a base
        // like "http://127.0.0.1:14196" + "api/session_connect" produced the
        // invalid URL "http://127.0.0.1:14196api/session_connect", so the
        // browser's XMLHttpRequest.open() threw and the WebRTC signaling POST
        // was never sent (handshake stuck on "Identity Token not set").
        if self.authority_end() == self.original.len() {
            write!(f, "/")?;
        }
        Ok(())
    }
}

impl Url {
    /// Byte offset just past the authority (`scheme://host:port`).
    /// Returns `original.len()` if there is no path/query/fragment.
    fn authority_end(&self) -> usize {
        let s = self.original.as_str();
        let after_scheme = s.find("//").map(|i| i + 2).unwrap_or(0);
        s[after_scheme..]
            .find(['/', '?', '#'])
            .map(|i| after_scheme + i)
            .unwrap_or(s.len())
    }

    pub fn path_segments(&self) -> Option<impl Iterator<Item = &str>> {
        let s = self.original.as_str();
        let auth_end = self.authority_end();
        if auth_end >= s.len() || s.as_bytes()[auth_end] != b'/' {
            return None;
        }
        let path = &s[auth_end + 1..];
        let path_end = path.find(['?', '#']).unwrap_or(path.len());
        Some(path[..path_end].split('/').filter(|seg| !seg.is_empty()))
    }

    pub fn query(&self) -> Option<&str> {
        let s = self.original.as_str();
        s.find('?').and_then(|i| {
            let q = &s[i + 1..];
            let q = q.find('#').map(|j| &q[..j]).unwrap_or(q);
            if q.is_empty() {
                None
            } else {
                Some(q)
            }
        })
    }

    pub fn fragment(&self) -> Option<&str> {
        let s = self.original.as_str();
        s.find('#').and_then(|i| {
            let f = &s[i + 1..];
            if f.is_empty() {
                None
            } else {
                Some(f)
            }
        })
    }

    pub fn scheme(&self) -> &str {
        let s = self.original.as_str();
        s.find("://").map(|i| &s[..i]).unwrap_or("")
    }

    /// Host string (without port), or empty string if not parseable.
    pub fn host_str(&self) -> &str {
        let s = self.original.as_str();
        let after_scheme = s.find("//").map(|i| i + 2).unwrap_or(0);
        let authority = &s[after_scheme..self.authority_end()];
        // Strip port: find last ':' that is after any ']' (IPv6 bracket close).
        let bracket_end = authority.rfind(']').unwrap_or(0);
        if let Some(colon) = authority[bracket_end..].rfind(':') {
            &authority[..bracket_end + colon]
        } else {
            authority
        }
    }

    /// Explicit port from the URL, or `None` if not present.
    pub fn port(&self) -> Option<u16> {
        let s = self.original.as_str();
        let after_scheme = s.find("//").map(|i| i + 2).unwrap_or(0);
        let authority = &s[after_scheme..self.authority_end()];
        let bracket_end = authority.rfind(']').unwrap_or(0);
        authority[bracket_end..]
            .rfind(':')
            .and_then(|colon| authority[bracket_end + colon + 1..].parse().ok())
    }
}

pub fn parse_server_url(server_url_str: &str) -> Url {
    let url = Url {
        original: server_url_str.to_string(),
    };

    if let Some(path_segments) = url.path_segments() {
        if path_segments.count() > 1 {
            panic!("server_url_str must not include a path (got: {server_url_str:?})");
        }
    }
    if url.query().is_some() {
        panic!("server_url_str must not include a query string (got: {server_url_str:?})");
    }
    if url.fragment().is_some() {
        panic!("server_url_str must not include a fragment (got: {server_url_str:?})");
    }

    url
}

cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        pub fn url_to_socket_addr(url: &Url) -> SocketAddr {
            use std::net::ToSocketAddrs;

            let host = url.host_str();
            let port = url.port().unwrap_or_else(|| match url.scheme() {
                "http" | "ws" => 80,
                "https" | "wss" => 443,
                _ => 0,
            });

            let addrs: Vec<SocketAddr> = format!("{}:{}", host, port)
                .to_socket_addrs()
                .unwrap_or_else(|err| {
                    panic!("could not resolve {host}:{port} to a SocketAddr: {err:?}");
                })
                .collect();

            if addrs.is_empty() {
                panic!("{host}:{port} resolved to no SocketAddr");
            }
            addrs[0]
        }
    } else {
        pub fn url_to_socket_addr(_url: &Url) -> SocketAddr {
            panic!("should not need this method for Wasm apps");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_server_url;

    // The session-URL builders concatenate base + endpoint path with no
    // separator (wasm_bindgen `format!("{}{}", base, path)`, miniquad
    // `address + rtc_path`), so a bare-authority base MUST Display with a
    // trailing "/". Regression guard for the `url` → minimal-parser swap that
    // dropped this and broke WebRTC signaling ("Identity Token not set").
    #[test]
    fn bare_authority_displays_with_trailing_slash() {
        assert_eq!(
            parse_server_url("http://127.0.0.1:14196").to_string(),
            "http://127.0.0.1:14196/"
        );
        // Concatenation with an endpoint path yields a valid URL.
        let base = parse_server_url("http://127.0.0.1:14196");
        assert_eq!(
            format!("{}{}", base, "api/session_connect"),
            "http://127.0.0.1:14196/api/session_connect"
        );
    }

    #[test]
    fn existing_trailing_slash_is_not_doubled() {
        assert_eq!(
            parse_server_url("http://127.0.0.1:14196/").to_string(),
            "http://127.0.0.1:14196/"
        );
    }

    #[test]
    fn single_path_segment_is_preserved() {
        // parse_server_url permits a single path segment; Display must not
        // append another slash.
        assert_eq!(
            parse_server_url("https://example.com/base").to_string(),
            "https://example.com/base"
        );
    }

    /// The rejection panics carry no information without the offending URL, which
    /// is the whole point of the check -- the caller has to know which of their
    /// config strings is wrong.
    #[test]
    #[should_panic(expected = "http://127.0.0.1:14196/some/path")]
    fn path_rejection_names_the_url() {
        parse_server_url("http://127.0.0.1:14196/some/path");
    }

    #[test]
    #[should_panic(expected = "http://127.0.0.1:14196/?a=b")]
    fn query_rejection_names_the_url() {
        parse_server_url("http://127.0.0.1:14196/?a=b");
    }

    #[test]
    #[should_panic(expected = "http://127.0.0.1:14196/#frag")]
    fn fragment_rejection_names_the_url() {
        parse_server_url("http://127.0.0.1:14196/#frag");
    }
}
