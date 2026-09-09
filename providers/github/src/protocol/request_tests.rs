#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::protocol::Github;
    fn parse(bytes: &[u8]) -> Result<(), ()> {
        permesh_native_runtime::validate_request::<Github>(bytes).map_err(|_| ())
    }
    #[test]
    fn strict_request_schemas_reject_duplicates_unknowns_nulls_and_wrong_method_fields() {
        let handshake =
            br#"{"protocol_version":1,"id":"handshake","method":"handshake","operation":"discover","instance":"github-main"}"#;
        assert!(parse(handshake).is_ok());
        for bytes in [
            &br#"{"protocol":2,"id":"handshake","method":"handshake","instance":"github-main"}"#[..],
            &br#"{"protocol_version":1,"protocol_version":1,"id":"handshake","method":"handshake","operation":"discover","instance":"github-main"}"#[..],
            &br#"{"protocol_version":1,"id":"handshake","method":"handshake","operation":"discover","instance":"github-main","unknown":"SECRET"}"#[..],
            &br#"{"protocol_version":1,"id":"handshake","method":"handshake","operation":"discover","instance":"github-main","configuration":null}"#[..],
            &br#"{"protocol_version":1,"id":"discover","method":"discover","configuration":{"organizations":["acme"],"endpoint":"SECRET"},"credentials":{"token":"SECRET"}}"#[..],
            &br#"{"protocol_version":1,"id":"check","method":"check","configuration":{"organizations":["acme"]},"credentials":{"token":"SECRET","token":"SECOND"}}"#[..],
            &br#"{"protocol_version":1,"id":"check","method":"check","configuration":{"organizations":["acme"]},"credentials":{"token":"SECRET","other":"SECOND"}}"#[..],
            &br#"{"protocol":3,"id":"describe","method":"describe","credentials":{"token":"SECRET"}}"#[..],
            &br#"{"protocol":3,"id":"discover","method":"discover","configuration":{"organizations":["acme"]},"credentials":{"token":"SECRET"}}"#[..],
            &br#"{"protocol":1,"id":"handshake","method":"handshake","instance":"github-main"}"#[..],
        ] {assert!(parse(bytes).is_err());}
    }
}
