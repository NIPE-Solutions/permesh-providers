// SPDX-License-Identifier: MIT
//! Single-operation discovery, setup and optional browser-auth descriptions.
mod io;
mod records;
mod request;
mod response;

use io::{Input, Output};
use permesh_provider_sdk::ProviderError;
use permesh_provider_sdk::{Metadata, Provider, browser_auth::BrowserAuthSpec, setup::SetupSpec};
use request::{Command, Operation, RequestContract};
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
    output.contract = request.contract;
    let Command::Handshake {
        instance,
        operation,
    } = request.command
    else {
        return Err(Failure::Protocol);
    };
    let metadata = A::metadata();
    let capabilities: Vec<_> = metadata
        .capabilities
        .into_iter()
        .map(records::capability)
        .collect();
    if output.contract == RequestContract::NegotiatedV1 {
        output.send(&serde_json::json!({"event":"handshake","provider":metadata.kind,"capabilities":capabilities,"operations":["discover","check"],"draft":true})).await?;
    } else {
        output.send(&serde_json::json!({"event":"handshake","provider":metadata.kind,"capabilities":capabilities,"draft":true})).await?;
    }
    let request = next::<A, _>(input).await?;
    if request.contract != output.contract {
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
        Command::DescribeAuth => {
            output.id = "describe_auth";
            let spec = A::browser_auth().ok_or(Failure::Provider("unsupported_method"))?;
            spec.validate().map_err(|_| Failure::Internal)?;
            output
                .send(&serde_json::json!({"event":"auth","spec":spec}))
                .await
        }
        Command::Cancel => Err(Failure::Cancelled),
        Command::Operation {
            check,
            configuration,
            credentials,
        } => {
            output.id = if check { "check" } else { "discover" };
            if operation
                != Some(if check {
                    Operation::Check
                } else {
                    Operation::Discover
                })
            {
                return Err(Failure::Protocol);
            }
            let contract = output.contract;
            // Read cancellation concurrently, with no task that can outlive this session.
            tokio::select! {
                biased;
                frame=input.frame()=>{
                    let frame=frame?;
                    let request=request::parse::<A>(&frame).map_err(|_|Failure::Protocol)?;
                    if request.contract==contract && matches!(request.command,Command::Cancel) {Err(Failure::Cancelled)} else {Err(Failure::Protocol)}
                },
                result=tokio::time::timeout(Duration::from_secs(55),async {
                    let provider = factory(instance.clone(), configuration, credentials).await.map_err(|e| Failure::Provider(A::error_code(&e.code)))?;
                    response::operation::<A, _, _>(&provider,check,&instance,output).await
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
    /// Optional credential-free description; login and keychain writes belong to the host.
    fn browser_auth() -> Option<BrowserAuthSpec> {
        None
    }
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
