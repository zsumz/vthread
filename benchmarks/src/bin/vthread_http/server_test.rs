use super::{Parsed, Responses, parse};

#[test]
fn partial_and_pipelined_requests_are_framed() {
    let responses = Responses::new();
    assert!(matches!(parse(b"GET /plain", &responses), Parsed::Partial));

    let requests = concat!(
        "GET /plaintext HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /json HTTP/1.1\r\nHost: localhost\r\n\r\n"
    );
    let first = parse(requests.as_bytes(), &responses);
    let Parsed::Complete {
        consumed,
        response,
        close,
    } = first
    else {
        panic!("first request was not complete");
    };
    assert!(response.ends_with(b"Hello, World!"));
    assert!(!close);
    assert!(matches!(
        parse(&requests.as_bytes()[consumed..], &responses),
        Parsed::Complete { response, .. }
            if response.ends_with(br#"{"message":"Hello, World!"}"#)
    ));
}

#[test]
fn body_and_connection_close_are_observed() {
    let responses = Responses::new();
    let request = b"POST /plaintext HTTP/1.1\r\nContent-Length: 4\r\nConnection: close\r\n\r\ndata";

    assert!(matches!(
        parse(request, &responses),
        Parsed::Complete {
            consumed,
            close: true,
            ..
        } if consumed == request.len()
    ));
}

#[test]
fn repeated_equal_lengths_and_connection_tokens_are_accepted() {
    let responses = Responses::new();
    let request = b"POST /json HTTP/1.1\r\nContent-Length: 0\r\nContent-Length: 0\r\nConnection: upgrade, close\r\n\r\n";

    assert!(matches!(
        parse(request, &responses),
        Parsed::Complete {
            consumed,
            close: true,
            ..
        } if consumed == request.len()
    ));
}

#[test]
fn close_token_wins_regardless_of_token_order() {
    let responses = Responses::new();
    for value in ["close, keep-alive", "keep-alive, close"] {
        let request = format!("GET / HTTP/1.1\r\nConnection: {value}\r\n\r\n");
        assert!(matches!(
            parse(request.as_bytes(), &responses),
            Parsed::Complete { close: true, .. }
        ));
    }
}

#[test]
fn http_1_0_closes_unless_keep_alive_is_explicit() {
    let responses = Responses::new();
    assert!(matches!(
        parse(b"GET / HTTP/1.0\r\n\r\n", &responses),
        Parsed::Complete { close: true, .. }
    ));
    assert!(matches!(
        parse(
            b"GET / HTTP/1.0\r\nConnection: keep-alive\r\n\r\n",
            &responses
        ),
        Parsed::Complete { close: false, .. }
    ));
}

#[test]
fn malformed_framing_is_rejected() {
    let responses = Responses::new();
    let conflicting = b"GET / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nxx";
    let chunked = b"GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n";

    assert!(matches!(parse(conflicting, &responses), Parsed::Malformed));
    assert!(matches!(parse(chunked, &responses), Parsed::Malformed));
}
