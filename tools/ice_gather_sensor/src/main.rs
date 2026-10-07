//! Logging forward proxy: CLIENT_PORT -> 127.0.0.1:REAL_PORT.
//!
//! Usage: session_proxy CLIENT_PORT REAL_PORT OUT_FILE
//!
//! Listens on 127.0.0.1:CLIENT_PORT and forwards one HTTP request per
//! connection to 127.0.0.1:REAL_PORT, relaying the response back. The body of
//! every POST that carries one rewrites OUT_FILE (the session POST offer is
//! what matters). Forwards bytes identically. Runs until killed; a bad
//! argument or a failed bind exits 1.

use std::{
    env, fs,
    io::{ErrorKind, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    thread,
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_HEAD: usize = 65536;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (client_port, real_port, out_file) = match parse_args(&args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("session_proxy: {error}");
            eprintln!("usage: session_proxy CLIENT_PORT REAL_PORT OUT_FILE");
            return ExitCode::from(1);
        }
    };
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, client_port)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("session_proxy: cannot bind 127.0.0.1:{client_port}: {error}");
            return ExitCode::from(1);
        }
    };
    match serve(listener, real_port, out_file) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("session_proxy: accept failed: {error}");
            ExitCode::from(1)
        }
    }
}

fn parse_args(args: &[String]) -> Result<(u16, u16, PathBuf), String> {
    let [client_port, real_port, out_file, ..] = args else {
        return Err("expected three arguments".to_string());
    };
    let port = |arg: &str| {
        arg.trim()
            .parse::<u16>()
            .map_err(|error| format!("bad port {arg:?}: {error}"))
    };
    Ok((
        port(client_port)?,
        port(real_port)?,
        PathBuf::from(out_file),
    ))
}

/// Accepts forever, handling each connection on its own thread. Returns only
/// when `accept` fails.
fn serve(listener: TcpListener, real_port: u16, out_file: PathBuf) -> std::io::Result<()> {
    let out_file = Arc::new(out_file);
    let upstream = SocketAddr::from((Ipv4Addr::LOCALHOST, real_port));
    loop {
        let (client, _) = listener.accept()?;
        let out_file = Arc::clone(&out_file);
        thread::spawn(move || handle(client, upstream, &out_file));
    }
}

/// One request through to the upstream and its response back. Any socket
/// error abandons the exchange and closes both ends.
fn handle(mut client: TcpStream, upstream: SocketAddr, out_file: &Path) {
    let _ = client.set_read_timeout(Some(TIMEOUT));
    let _ = client.set_write_timeout(Some(TIMEOUT));
    let Some((head, body)) = read_http_message(&mut client) else {
        return;
    };
    // The capture comes first: a capture file that cannot be written abandons
    // the exchange, so a run never forwards an offer it failed to record.
    if head.starts_with(b"POST") && !body.is_empty() && fs::write(out_file, &body).is_err() {
        return;
    }
    let _ = forward(&mut client, upstream, &head, &body);
}

fn forward(
    client: &mut TcpStream,
    upstream: SocketAddr,
    head: &[u8],
    body: &[u8],
) -> std::io::Result<()> {
    let mut server = TcpStream::connect_timeout(&upstream, TIMEOUT)?;
    server.set_read_timeout(Some(TIMEOUT))?;
    server.set_write_timeout(Some(TIMEOUT))?;
    server.write_all(head)?;
    server.write_all(body)?;
    if let Some((response_head, response_body)) = read_http_message(&mut server) {
        client.write_all(&response_head)?;
        client.write_all(&response_body)?;
    }
    Ok(())
}

/// Reads an HTTP head (through the blank line) and its body: exactly
/// `Content-Length` bytes when the head names one, else everything until EOF
/// or the read timeout. `None` when the head never completes: EOF, timeout,
/// error, or more than 64 KiB of head. A body cut short by EOF or an error is
/// returned as far as it got.
fn read_http_message(stream: &mut impl Read) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => return None,
            Ok(_) => head.push(byte[0]),
        }
        if head.len() > MAX_HEAD {
            return None;
        }
    }
    let mut body = Vec::new();
    match content_length(&head) {
        Some(length) => {
            while body.len() < length {
                let mut chunk = vec![0u8; length - body.len()];
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => body.extend_from_slice(&chunk[..n]),
                }
            }
        }
        None => {
            let mut chunk = [0u8; 4096];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => body.extend_from_slice(&chunk[..n]),
                    Err(error) if error.kind() == ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        }
    }
    Some((head, body))
}

/// The last `Content-Length` header's value. A value that is not an integer
/// counts as 0; a negative one reads no body.
fn content_length(head: &[u8]) -> Option<usize> {
    // Latin-1: every byte is one char, so no header byte is ever lost.
    let head: String = head.iter().map(|&b| b as char).collect();
    let mut length = None;
    for line in head.split("\r\n") {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case("content-length") {
                length = Some(parse_int(value.trim()).unwrap_or(0));
            }
        }
    }
    length.map(|length| usize::try_from(length).unwrap_or(0))
}

/// A decimal integer with an optional sign, allowing single `_` separators
/// between digits (`1_000`).
fn parse_int(text: &str) -> Option<i64> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return None;
    }
    text.replace('_', "").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn content_length_body_is_read_exactly() {
        let mut wire = Cursor::new(b"POST /s HTTP/1.1\r\nContent-Length: 3\r\n\r\nabcdef".to_vec());
        let (head, body) = read_http_message(&mut wire).unwrap();
        assert_eq!(head, b"POST /s HTTP/1.1\r\nContent-Length: 3\r\n\r\n");
        assert_eq!(body, b"abc");
    }

    #[test]
    fn body_without_length_runs_to_eof() {
        let mut wire = Cursor::new(b"HTTP/1.1 200 OK\r\n\r\nrest of it".to_vec());
        assert_eq!(read_http_message(&mut wire).unwrap().1, b"rest of it");
    }

    #[test]
    fn short_body_returns_what_arrived() {
        let mut wire = Cursor::new(b"POST / HTTP/1.1\r\ncontent-length: 9\r\n\r\nab".to_vec());
        assert_eq!(read_http_message(&mut wire).unwrap().1, b"ab");
    }

    #[test]
    fn incomplete_or_oversized_heads_are_refused() {
        assert!(read_http_message(&mut Cursor::new(b"GET / HTTP/1.1\r\n".to_vec())).is_none());
        let mut huge = b"GET / HTTP/1.1\r\nX: ".to_vec();
        huge.extend(std::iter::repeat_n(b'a', MAX_HEAD));
        huge.extend_from_slice(b"\r\n\r\n");
        assert!(read_http_message(&mut Cursor::new(huge)).is_none());
    }

    #[test]
    fn content_length_takes_the_last_header_and_forgives_garbage() {
        assert_eq!(
            content_length(b"A: 1\r\nContent-Length: 4\r\ncontent-length: 2\r\n\r\n"),
            Some(2)
        );
        assert_eq!(content_length(b"Content-Length: x\r\n\r\n"), Some(0));
        assert_eq!(content_length(b"Content-Length: -5\r\n\r\n"), Some(0));
        assert_eq!(content_length(b"Host: a\r\n\r\n"), None);
        assert_eq!(content_length(b"Content-Length: 1_0\r\n\r\n"), Some(10));
        assert_eq!(content_length(b"Content-Length: 1__0\r\n\r\n"), Some(0));
    }

    #[test]
    fn unwritable_capture_abandons_the_exchange() {
        let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let real_port = upstream.local_addr().unwrap().port();
        upstream.set_nonblocking(true).unwrap();

        let proxy = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client_port = proxy.local_addr().unwrap().port();
        let unwritable = env::temp_dir().join("naia-session-proxy-missing-dir/offer.txt");
        thread::spawn(move || serve(proxy, real_port, unwritable));

        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, client_port)).unwrap();
        client.set_read_timeout(Some(TIMEOUT)).unwrap();
        client
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\noffer")
            .unwrap();
        let mut received = Vec::new();
        client.read_to_end(&mut received).unwrap();
        assert!(received.is_empty());
        assert_eq!(
            upstream.accept().map(|_| ()).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
    }

    #[test]
    fn args_need_two_ports_and_a_file() {
        assert!(parse_args(&["1".into(), "2".into()]).is_err());
        assert!(parse_args(&["x".into(), "2".into(), "f".into()]).is_err());
        assert_eq!(
            parse_args(&["14201".into(), "14191".into(), "/tmp/o".into()]).unwrap(),
            (14201, 14191, PathBuf::from("/tmp/o"))
        );
    }

    /// End to end over real loopback sockets: the POST body is captured, the
    /// upstream sees the request byte for byte, and the client gets the
    /// upstream's response byte for byte.
    #[test]
    fn proxies_one_exchange_and_captures_the_post_body() {
        let request = b"POST /rtc_session HTTP/1.1\r\nContent-Length: 5\r\n\r\noffer".to_vec();
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nanswer".to_vec();

        let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let real_port = upstream.local_addr().unwrap().port();
        let upstream_response = response.clone();
        let upstream_thread = thread::spawn(move || {
            let (mut conn, _) = upstream.accept().unwrap();
            conn.set_read_timeout(Some(TIMEOUT)).unwrap();
            let (head, body) = read_http_message(&mut conn).unwrap();
            conn.write_all(&upstream_response).unwrap();
            [head, body].concat()
        });

        let out_file = env::temp_dir().join(format!("naia-session-proxy-{}", std::process::id()));
        let _ = fs::remove_file(&out_file);
        let proxy = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client_port = proxy.local_addr().unwrap().port();
        let proxy_out = out_file.clone();
        thread::spawn(move || serve(proxy, real_port, proxy_out));

        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, client_port)).unwrap();
        client.set_read_timeout(Some(TIMEOUT)).unwrap();
        client.write_all(&request).unwrap();
        let mut received = Vec::new();
        client.read_to_end(&mut received).unwrap();

        assert_eq!(upstream_thread.join().unwrap(), request);
        assert_eq!(received, response);
        assert_eq!(fs::read(&out_file).unwrap(), b"offer");
        let _ = fs::remove_file(&out_file);
    }
}
