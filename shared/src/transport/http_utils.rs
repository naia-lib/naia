use http::Method;
use log::warn;

#[doc(hidden)]
pub fn request_to_bytes(request: http::Request<Vec<u8>>) -> Vec<u8> {
    let url = request.uri();

    let mut request_string = format!("{} {} HTTP/1.1\r\n", request.method(), url.path(),);

    // Add the Host header
    if let Some(host) = url.host() {
        request_string.push_str(&format!("Host: {}\r\n", host));
    }

    // Add the other headers
    for (key, value) in request.headers() {
        let Ok(value_str) = value.to_str() else {
            warn!(
                "Failed to convert header `{:?}`'s value to string: {:?}",
                key, value
            );
            continue;
        };
        request_string.push_str(&format!("{}: {}\r\n", key, value_str));
    }

    // Add a blank line to indicate the end of headers
    request_string.push_str("\r\n");

    let mut request_bytes = request_string.into_bytes();

    // Add the body
    if !request.body().is_empty() {
        request_bytes.extend_from_slice(request.body());
    }

    request_bytes
}

/// Why a byte string is not a parseable HTTP request.
///
/// Returned by [`bytes_to_request`] instead of panicking. Every variant maps to
/// the pre-existing generic malformed-request behavior at the call sites: the
/// request is dropped without a fingerprint verdict, an auth decode, a user
/// allocation, or any response that could be mistaken for a mismatch answer.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestParseError {
    /// Fewer than the three request-line parts (`METHOD path VERSION`).
    BadRequestLine,
    /// The method is not a valid HTTP method.
    BadMethod,
    /// The request target is not a valid URI.
    BadUri,
    /// A header line has no `: ` separator.
    BadHeader,
    /// The parsed parts do not form a valid request.
    Rejected,
}

#[doc(hidden)]
pub fn bytes_to_request(request_bytes: &[u8]) -> Result<http::Request<Vec<u8>>, RequestParseError> {
    let request_str = String::from_utf8_lossy(request_bytes);

    let (path_line, headers_str, body_start_index) = split_request(&request_str);
    let (method, url) = parse_path_line(path_line)?;
    let headers = parse_headers(headers_str)?;
    let body = request_bytes[body_start_index..].to_vec();

    let mut request = http::Request::builder().method(method).uri(url);
    for (key, value) in headers {
        request = request.header(key, value);
    }
    request.body(body).map_err(|_| RequestParseError::Rejected)
}

#[doc(hidden)]
pub fn response_to_bytes(response: http::Response<Vec<u8>>) -> Vec<u8> {
    let mut response_bytes = response_header_to_vec(&response);
    response_bytes.extend_from_slice(response.body());
    response_bytes
}

#[doc(hidden)]
pub fn bytes_to_response(response_bytes: &[u8]) -> http::Response<Vec<u8>> {
    let response_str = String::from_utf8_lossy(response_bytes);

    let (status_line, headers_str, body_start_index) = split_response(&response_str);
    let (status_code, _status_text) = parse_status_line(status_line);
    // Response parsing keeps its historical tolerance: a malformed header
    // block yields no headers rather than a failure. Only request parsing
    // (bytes_to_request) reports typed errors.
    let headers = parse_headers(headers_str).unwrap_or_default();
    let body = response_bytes[body_start_index..].to_vec();

    let mut response = http::Response::builder().status(status_code);
    for (key, value) in headers {
        response = response.header(key, value);
    }
    response.body(body).unwrap()
}

fn split_request(request_str: &str) -> (&str, &str, usize) {
    let mut parts = request_str.splitn(3, "\r\n\r\n");
    let status_and_headers = parts.next().unwrap();
    let mut pathline_and_headers_parts = status_and_headers.splitn(2, "\r\n");
    let path_line = pathline_and_headers_parts.next().unwrap();
    let headers = pathline_and_headers_parts.next().unwrap_or(""); // If there are no headers, it's an empty string
    let body_start_index = request_str
        .find("\r\n\r\n")
        .map(|idx| idx + 4)
        .unwrap_or(request_str.len());
    (path_line, headers, body_start_index)
}

fn split_response(response_str: &str) -> (&str, &str, usize) {
    let mut parts = response_str.splitn(3, "\r\n\r\n");
    let status_and_headers = parts.next().unwrap();
    let mut status_and_headers_parts = status_and_headers.splitn(2, "\r\n");
    let status_line = status_and_headers_parts.next().unwrap();
    let headers = status_and_headers_parts.next().unwrap_or(""); // If there are no headers, it's an empty string
    let body_start_index = response_str
        .find("\r\n\r\n")
        .map(|idx| idx + 4)
        .unwrap_or(response_str.len());
    (status_line, headers, body_start_index)
}

fn parse_path_line(path_line: &str) -> Result<(Method, String), RequestParseError> {
    let mut parts = path_line.splitn(3, ' ');
    let (Some(method), Some(path), Some(_http_version)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err(RequestParseError::BadRequestLine);
    };

    let method = method
        .parse::<Method>()
        .map_err(|_| RequestParseError::BadMethod)?;
    path.parse::<http::Uri>()
        .map_err(|_| RequestParseError::BadUri)?;
    Ok((method, path.to_string()))
}

fn parse_status_line(status_line: &str) -> (u16, String) {
    let mut parts = status_line.splitn(3, ' ');
    let _http_version = parts.next().unwrap();
    let status_code = parts.next().unwrap();
    let status_text = parts.next().unwrap_or("").to_string(); // Status text can be empty
    let status_code = status_code.parse::<u16>().unwrap();
    (status_code, status_text)
}

fn parse_headers(headers: &str) -> Result<Vec<(String, String)>, RequestParseError> {
    let mut header_store: Vec<(String, String)> = Vec::new();
    for line in headers.lines() {
        let mut parts = line.splitn(2, ": ");
        let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
            return Err(RequestParseError::BadHeader);
        };
        header_store.push((key.to_lowercase(), value.to_string()));
    }
    Ok(header_store)
}

fn response_header_to_vec(r: &http::Response<Vec<u8>>) -> Vec<u8> {
    let mut v = Vec::with_capacity(120);
    write_response_header(r, &mut v).expect("unable to write response header to stream");
    v
}

fn write_response_header(r: &http::Response<Vec<u8>>, output: &mut Vec<u8>) -> std::io::Result<()> {
    let status = r.status().as_u16();
    let code = status.to_string();

    write_line(b"HTTP/1.1 ", output)?;
    write_line(code.as_bytes(), output)?;
    write_line(b"\r\n", output)?;

    for (hn, hv) in r.headers() {
        let Ok(hv) = hv.to_str() else {
            warn!(
                "Failed to convert header `{:?}`'s value to string: {:?}",
                hn, hv
            );
            continue;
        };

        // info!("writing header: {}: {}", hn, hv);
        write_line(hn.as_str().as_bytes(), output)?;
        write_line(b": ", output)?;
        write_line(hv.as_bytes(), output)?;
        write_line(b"\r\n", output)?;
    }

    write_line(b"\r\n", output)?;

    Ok(())
}

fn write_line(buf: &[u8], io: &mut dyn std::io::Write) -> std::io::Result<()> {
    io.write_all(buf)?;
    Ok(())
}
