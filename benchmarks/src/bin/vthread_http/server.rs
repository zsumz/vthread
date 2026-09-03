use std::{net::SocketAddr, sync::Arc, time::SystemTime};
use vthread::{
    Error, Result, Scope, Spawner,
    net::{TcpListener, TcpStream},
};

const INPUT_CAPACITY: usize = 32 * 1024;
const OUTPUT_CAPACITY: usize = 64 * 1024;
const MAX_HEADERS: usize = 16;

struct Responses {
    plaintext: Vec<u8>,
    json: Vec<u8>,
    not_found: Vec<u8>,
    bad_request: Vec<u8>,
}

enum Parsed<'a> {
    Complete {
        consumed: usize,
        response: &'a [u8],
        close: bool,
    },
    Partial,
    Malformed,
}

pub(super) fn run(scope: &Scope<'_>, address: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(address)?;
    let address = listener.local_addr()?;
    let responses = Arc::new(Responses::new());
    let spawner = scope.spawner();
    eprintln!("vthread-http listening on {address}");
    let mut accept = scope.spawn("http-accept", move || {
        accept_loop(listener, spawner, responses)
    })?;
    accept.join()?
}

fn accept_loop(listener: TcpListener, spawner: Spawner, responses: Arc<Responses>) -> Result<()> {
    loop {
        let (stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(Error::Cancelled | Error::RuntimeStopped) => return Ok(()),
            Err(error) => return Err(error),
        };
        let responses = Arc::clone(&responses);
        match spawner.spawn("http-connection", move || {
            let _ = serve(stream, &responses);
        }) {
            Ok(handle) => drop(handle),
            Err(Error::Capacity { .. }) => continue,
            Err(Error::Cancelled | Error::RuntimeStopped | Error::ScopeClosed) => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

fn serve(stream: TcpStream, responses: &Responses) -> Result<()> {
    let mut input = vec![0_u8; INPUT_CAPACITY];
    let mut output = Vec::with_capacity(OUTPUT_CAPACITY);
    let (mut start, mut end) = (0, 0);
    loop {
        let mut needs_read = false;
        loop {
            match parse(&input[start..end], responses) {
                Parsed::Complete {
                    consumed,
                    response,
                    close,
                } => {
                    if output.len() + response.len() > OUTPUT_CAPACITY {
                        break;
                    }
                    output.extend_from_slice(response);
                    start += consumed;
                    if close {
                        stream.write_all(&output)?;
                        return Ok(());
                    }
                }
                Parsed::Partial => {
                    needs_read = true;
                    break;
                }
                Parsed::Malformed => {
                    if !output.is_empty() {
                        stream.write_all(&output)?;
                    }
                    stream.write_all(&responses.bad_request)?;
                    return Ok(());
                }
            }
        }
        if !output.is_empty() {
            stream.write_all(&output)?;
            output.clear();
        }
        if !needs_read {
            continue;
        }
        if start == end {
            (start, end) = (0, 0);
        } else if end == input.len() && start != 0 {
            input.copy_within(start..end, 0);
            end -= start;
            start = 0;
        } else if end == input.len() {
            return Ok(());
        }
        let count = stream.read(&mut input[end..])?;
        if count == 0 {
            return Ok(());
        }
        end += count;
        while end < input.len() {
            match stream.try_read(&mut input[end..]) {
                Ok(0) => return Ok(()),
                Ok(count) => end += count,
                Err(Error::WouldBlock) => break,
                Err(error) => return Err(error),
            }
        }
    }
}

fn parse<'a>(input: &[u8], responses: &'a Responses) -> Parsed<'a> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut request = httparse::Request::new(&mut headers);
    let header_bytes = match request.parse(input) {
        Ok(httparse::Status::Complete(consumed)) => consumed,
        Ok(httparse::Status::Partial) => return Parsed::Partial,
        Err(_) => return Parsed::Malformed,
    };
    let body_bytes = match content_length(request.headers) {
        Some(length) => length,
        None => return Parsed::Malformed,
    };
    let Some(consumed) = header_bytes.checked_add(body_bytes) else {
        return Parsed::Malformed;
    };
    if consumed > input.len() {
        return Parsed::Partial;
    }
    let response = match request.path {
        Some("/plaintext") => &responses.plaintext,
        Some("/json") => &responses.json,
        _ => &responses.not_found,
    };
    Parsed::Complete {
        consumed,
        response,
        close: should_close(request.version, request.headers),
    }
}

fn content_length(headers: &[httparse::Header<'_>]) -> Option<usize> {
    let mut content_length = None;
    for header in headers {
        if header.name.eq_ignore_ascii_case("transfer-encoding") {
            return None;
        }
        if header.name.eq_ignore_ascii_case("content-length") {
            let value = std::str::from_utf8(header.value).ok()?.parse().ok()?;
            if content_length.is_some_and(|previous| previous != value) {
                return None;
            }
            content_length = Some(value);
        }
    }
    Some(content_length.unwrap_or(0))
}

fn should_close(version: Option<u8>, headers: &[httparse::Header<'_>]) -> bool {
    let mut close = false;
    let mut keep_alive = false;
    for header in headers {
        if header.name.eq_ignore_ascii_case("connection") {
            for value in header.value.split(|byte| *byte == b',').map(trim_ascii) {
                if value.eq_ignore_ascii_case(b"close") {
                    close = true;
                } else if value.eq_ignore_ascii_case(b"keep-alive") {
                    keep_alive = true;
                }
            }
        }
    }
    close || (version == Some(0) && !keep_alive)
}

fn trim_ascii(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

impl Responses {
    fn new() -> Self {
        let date = httpdate::fmt_http_date(SystemTime::now());
        Self {
            plaintext: response("200 Ok", "text/plain", b"Hello, World!", &date),
            json: response(
                "200 Ok",
                "application/json",
                br#"{"message":"Hello, World!"}"#,
                &date,
            ),
            not_found: response("404 Not Found", "text/plain", b"", &date),
            bad_request: response("400 Bad Request", "text/plain", b"", &date),
        }
    }
}

fn response(status: &str, content_type: &str, body: &[u8], date: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nServer: V\r\nDate: {date}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\n\r\n",
        body.len()
    )
    .bytes()
    .chain(body.iter().copied())
    .collect()
}

#[cfg(test)]
#[path = "server_test.rs"]
mod server_test;
