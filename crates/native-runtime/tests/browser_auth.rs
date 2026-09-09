// SPDX-License-Identifier: MIT
use permesh_core::Snapshot;
use permesh_native_runtime::{Adapter, serve};
use permesh_provider_protocol::{BrowserAuthDecoder, Progress, ProtocolError};
use permesh_provider_sdk::{Health, Metadata, Provider, ProviderFuture, setup::SetupSpec};

// GitHub, Cloudflare and AWS all use this default Adapter::browser_auth path.
struct WithoutBrowserAuth;
impl Adapter for WithoutBrowserAuth {
    type Configuration = ();
    type Credentials = ();
    fn metadata() -> Metadata {
        Metadata {
            kind: "without-browser-auth".into(),
            capabilities: vec![],
        }
    }
    fn setup() -> SetupSpec {
        panic!("browser-auth descriptions must not request setup")
    }
    fn validate(_: &(), _: &()) -> bool {
        false
    }
    fn error_code(_: &str) -> &'static str {
        "internal"
    }
    fn limitations(_: &[String]) -> Vec<&'static str> {
        vec![]
    }
}
impl Provider for WithoutBrowserAuth {
    fn metadata(&self) -> Metadata {
        <Self as Adapter>::metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        panic!("browser-auth descriptions must not check credentials")
    }
    fn discover(&self) -> ProviderFuture<'_, Snapshot> {
        panic!("browser-auth descriptions must not discover records")
    }
}

#[tokio::test]
async fn absent_browser_auth_cross_decodes_as_provider_failure_not_invalid_schema()
-> Result<(), Box<dyn std::error::Error>> {
    let input = b"{\"protocol\":4,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"test\"}\n{\"protocol\":4,\"id\":\"describe_auth\",\"method\":\"describe_auth\"}\n";
    let mut output = Vec::new();
    let result = serve::<WithoutBrowserAuth, _, _, _, _, WithoutBrowserAuth>(
        &input[..],
        &mut output,
        |_, _, _| async { panic!("descriptions must not construct a provider") },
    )
    .await;
    assert!(result.is_err());
    let frames: Vec<_> = output.split_inclusive(|b| *b == b'\n').collect();
    assert_eq!(frames.len(), 2);
    let mut decoder = BrowserAuthDecoder::new("without-browser-auth", "test", Some(&[]))?;
    assert_eq!(decoder.push_frame(frames[0])?, Progress::Handshake);
    assert_eq!(
        decoder.push_frame(frames[1]),
        Err(ProtocolError::ProviderFailed)
    );
    assert!(matches!(
        decoder.finish(),
        Err(ProtocolError::ProviderFailed)
    ));
    let error: serde_json::Value = serde_json::from_slice(frames[1])?;
    assert_eq!(error["code"], "unsupported_method");
    Ok(())
}

#[test]
fn negotiated_handshake_requires_one_explicit_family_and_supported_operation() {
    use permesh_native_runtime::validate_request;
    for operation in ["check", "discover"] {
        let frame = format!(
            r#"{{"protocol_version":1,"id":"handshake","method":"handshake","instance":"test","operation":"{operation}"}}"#
        );
        assert!(validate_request::<WithoutBrowserAuth>(frame.as_bytes()).is_ok());
    }
    for frame in [
        r#"{"protocol":2,"id":"handshake","method":"handshake","instance":"test"}"#,
        r#"{"protocol_version":1,"id":"handshake","method":"handshake","instance":"test"}"#,
        r#"{"protocol_version":1,"protocol":3,"id":"handshake","method":"handshake","instance":"test","operation":"check"}"#,
        r#"{"protocol_version":null,"id":"handshake","method":"handshake","instance":"test","operation":"check"}"#,
        r#"{"protocol_version":1,"protocol_version":1,"id":"handshake","method":"handshake","instance":"test","operation":"check"}"#,
        r#"{"protocol_version":1,"id":"handshake","method":"handshake","instance":"test","operation":"describe"}"#,
        r#"{"protocol":3,"id":"handshake","method":"handshake","instance":"test","operation":"check"}"#,
        r#"{"protocol_version":1,"id":"handshake","method":"handshake","instance":"test","operation":null}"#,
    ] {
        assert!(
            validate_request::<WithoutBrowserAuth>(frame.as_bytes()).is_err(),
            "{frame}"
        );
    }
}
