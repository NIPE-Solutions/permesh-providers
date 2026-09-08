// SPDX-License-Identifier: MIT OR Apache-2.0
//! Single-operation draft2 discovery/health and draft3 setup description.
mod io;
mod request;
mod response;

use io::{Input, Output};
use permesh_provider_sdk::ProviderError;
use permesh_provider_sdk::{Metadata, Provider, setup::SetupSpec};
use request::Command;
use serde::de::DeserializeOwned;
use std::future::Future;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
const MAX_FRAME: usize = 1_048_576;
#[derive(Debug)]
pub struct ProtocolFailure;
impl std::fmt::Display for ProtocolFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Native provider protocol failed")
    }
}
impl std::error::Error for ProtocolFailure {}
#[derive(Clone, Copy, Debug)]
enum Failure {
    Protocol,
    Internal,
    Unavailable,
    Cancelled,
    Provider(&'static str),
}
impl Failure {
    fn code(self) -> &'static str {
        match self {
            Self::Protocol => "protocol_error",
            Self::Internal => "internal",
            Self::Unavailable => "unavailable",
            Self::Cancelled => "internal",
            Self::Provider(code) => code,
        }
    }
}
async fn next<A: Adapter, R: AsyncRead + Unpin>(
    input: &mut Input<R>,
) -> Result<request::Request<A>, Failure> {
    let frame = tokio::time::timeout(Duration::from_secs(10), input.frame())
        .await
        .map_err(|_| Failure::Unavailable)??;
    request::parse(&frame).map_err(|_| Failure::Protocol)
}
pub async fn serve<A, R, W, F, Fut, P>(
    reader: R,
    writer: W,
    factory: F,
) -> Result<(), ProtocolFailure>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    A: Adapter,
    F: FnOnce(String, A::Configuration, A::Credentials) -> Fut,
    Fut: Future<Output = Result<P, ProviderError>>,
    P: Provider,
{
    let mut input = Input::new(reader);
    let mut output = Output::new(writer);
    let result = session::<A, _, _, _, _, _>(&mut input, &mut output, factory).await;
    match result {
        Ok(()) => Ok(()),
        Err(Failure::Cancelled) => {
            output.id = "cancel";
            output
                .send(&serde_json::json!({"event":"cancelled"}))
                .await
                .map_err(|_| ProtocolFailure)
        }
        Err(failure) => {
            // A broken or blocked output channel cannot deliver the static failure.
            if output
                .send(&serde_json::json!({"event":"error","code":failure.code()}))
                .await
                .is_err()
            {
                return Err(ProtocolFailure);
            }
            Err(ProtocolFailure)
        }
    }
}
async fn session<A, R, W, F, Fut, P>(
    input: &mut Input<R>,
    output: &mut Output<W>,
    factory: F,
) -> Result<(), Failure>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    A: Adapter,
    F: FnOnce(String, A::Configuration, A::Credentials) -> Fut,
    Fut: Future<Output = Result<P, ProviderError>>,
    P: Provider,
{
    let request = next::<A, _>(input).await?;
    output.version = request.protocol;
    let Command::Handshake { instance } = request.command else {
        return Err(Failure::Protocol);
    };
    output.send(&serde_json::json!({"event":"handshake","provider":A::metadata().kind,"capabilities":A::metadata().capabilities,"draft":true})).await?;
    let request = next::<A, _>(input).await?;
    if request.protocol != output.version {
        return Err(Failure::Protocol);
    }
    match request.command {
        Command::Describe => {
            output.id = "describe";
            let spec = A::setup();
            spec.validate().map_err(|_| Failure::Internal)?;
            output
                .send(&serde_json::json!({"event":"setup","spec":spec}))
                .await
        }
        Command::Cancel => Err(Failure::Cancelled),
        Command::Operation {
            check,
            configuration,
            credentials,
        } => {
            output.id = if check { "check" } else { "discover" };
            let version = output.version;
            // Read cancellation concurrently, with no task that can outlive this session.
            tokio::select! {
                biased;
                frame=input.frame()=>{
                    let frame=frame?;
                    let request=request::parse::<A>(&frame).map_err(|_|Failure::Protocol)?;
                    if request.protocol==version && matches!(request.command,Command::Cancel) {Err(Failure::Cancelled)} else {Err(Failure::Protocol)}
                },
                result=tokio::time::timeout(Duration::from_secs(55),async {
                    let provider = factory(instance, configuration, credentials).await.map_err(|e| Failure::Provider(A::error_code(&e.code)))?;
                    response::operation::<A, _, _>(&provider,check,output).await
                })=>result.map_err(|_|Failure::Unavailable)?,
            }
        }
        Command::Handshake { .. } => Err(Failure::Protocol),
    }
}
/// Provider-specific schemas and static, redacted presentation mappings.
pub trait Adapter {
    type Configuration: DeserializeOwned;
    type Credentials: DeserializeOwned;
    fn metadata() -> Metadata;
    fn setup() -> SetupSpec;
    fn validate(configuration: &Self::Configuration, credentials: &Self::Credentials) -> bool;
    fn error_code(code: &str) -> &'static str;
    fn limitations(values: &[String]) -> Vec<&'static str>;
}
/// Validate without retaining a credential-bearing frame or parsed request.
pub fn validate_request<A: Adapter>(bytes: &[u8]) -> Result<(), ProtocolFailure> {
    request::parse::<A>(bytes)
        .map(|_| ())
        .map_err(|_| ProtocolFailure)
}
