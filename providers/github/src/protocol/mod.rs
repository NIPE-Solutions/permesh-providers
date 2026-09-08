// SPDX-License-Identifier: MIT OR Apache-2.0
//! Single-operation draft2 discovery/health and draft3 setup description.
mod io;
mod request;
mod response;
mod setup;
use crate::GithubProvider;
use io::{Input, Output};
use permesh_provider_sdk::ProviderError;
use permesh_secrets::Secret;
use request::Command;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
const MAX_FRAME: usize = 1_048_576;
#[derive(Debug)]
pub struct ProtocolFailure;
impl std::fmt::Display for ProtocolFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitHub provider protocol failed")
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
async fn next<R: AsyncRead + Unpin>(input: &mut Input<R>) -> Result<request::Request, Failure> {
    let frame = tokio::time::timeout(Duration::from_secs(10), input.frame())
        .await
        .map_err(|_| Failure::Unavailable)??;
    request::parse(&frame).map_err(|_| Failure::Protocol)
}
/// Serve one explicit host invocation. No configuration or credentials are read
/// from the environment, arguments, files, or a workspace.
pub async fn run<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reader: R,
    writer: W,
) -> Result<(), ProtocolFailure> {
    serve(reader, writer, GithubProvider::new).await
}
async fn serve<R, W, F>(reader: R, writer: W, factory: F) -> Result<(), ProtocolFailure>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnOnce(String, Vec<String>, Secret) -> Result<GithubProvider, ProviderError>,
{
    let mut input = Input::new(reader);
    let mut output = Output::new(writer);
    let result = session(&mut input, &mut output, factory).await;
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
async fn session<R, W, F>(
    input: &mut Input<R>,
    output: &mut Output<W>,
    factory: F,
) -> Result<(), Failure>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnOnce(String, Vec<String>, Secret) -> Result<GithubProvider, ProviderError>,
{
    let request = next(input).await?;
    output.version = request.protocol;
    let Command::Handshake { instance } = request.command else {
        return Err(Failure::Protocol);
    };
    output.send(&serde_json::json!({"event":"handshake","provider":"github","capabilities":crate::provider_metadata().capabilities,"draft":true})).await?;
    let request = next(input).await?;
    if request.protocol != output.version {
        return Err(Failure::Protocol);
    }
    match request.command {
        Command::Describe => {
            output.id = "describe";
            let spec = setup::spec();
            spec.validate().map_err(|_| Failure::Internal)?;
            output
                .send(&serde_json::json!({"event":"setup","spec":spec}))
                .await
        }
        Command::Cancel => Err(Failure::Cancelled),
        Command::Operation {
            check,
            configuration,
            mut credentials,
        } => {
            output.id = if check { "check" } else { "discover" };
            let token = Secret::new(std::mem::take(&mut *credentials.token));
            let provider = factory(instance, configuration.organizations, token)
                .map_err(|e| Failure::Provider(response::error_code(&e.code)))?;
            let version = output.version;
            // Read cancellation concurrently, with no task that can outlive this session.
            tokio::select! {
                biased;
                frame=input.frame()=>{
                    let frame=frame?;
                    let request=request::parse(&frame).map_err(|_|Failure::Protocol)?;
                    if request.protocol==version && matches!(request.command,Command::Cancel) {Err(Failure::Cancelled)} else {Err(Failure::Protocol)}
                },
                result=tokio::time::timeout(Duration::from_secs(55),response::operation(&provider,check,output))=>result.map_err(|_|Failure::Unavailable)?,
            }
        }
        Command::Handshake { .. } => Err(Failure::Protocol),
    }
}
#[cfg(test)]
mod tests;
