/// MatchFunc allows custom logic for mapping packets to an Endpoint
pub(crate) type MatchFunc = Box<dyn (Fn(&[u8]) -> bool) + Send + Sync>;

/// match_range is a MatchFunc that accepts packets with the first byte in [lower..upper]
pub(crate) fn match_range(lower: u8, upper: u8) -> MatchFunc {
    Box::new(move |buf: &[u8]| -> bool {
        if buf.is_empty() {
            return false;
        }
        let b = buf[0];
        b >= lower && b <= upper
    })
}

/// MatchFuncs as described in RFC7983
/// <https://tools.ietf.org/html/rfc7983>
///              +----------------+
///              |        [0..3] -+--> forward to STUN
///              |                |
///              |      [16..19] -+--> forward to ZRTP
///              |                |
///  packet -->  |      [20..63] -+--> forward to DTLS
///              |                |
///              |      [64..79] -+--> forward to TURN Channel
///              |                |
///              |    [128..191] -+--> forward to RTP/RTCP
///              +----------------+
/// match_dtls is a MatchFunc that accepts packets with the first byte in [20..63]
/// as defied in RFC7983
pub(crate) fn match_dtls(b: &[u8]) -> bool {
    match_range(20, 63)(b)
}
