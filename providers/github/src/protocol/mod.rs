// SPDX-License-Identifier: MIT
//! Native GitHub provider protocol, using the shared bounded runtime.
mod request;
mod response;
mod setup;
use crate::GithubProvider;
use permesh_native_runtime::Adapter;
pub use permesh_native_runtime::ProtocolFailure;
use permesh_provider_sdk::{Metadata, ProviderError, setup::SetupSpec};
use permesh_secrets::Secret;
#[cfg(test)]
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
struct Github;
impl Adapter for Github {
    type Configuration = request::Configuration;
    type Credentials = request::Credentials;
    fn metadata() -> Metadata {
        crate::provider_metadata()
    }
    fn setup() -> SetupSpec {
        setup::spec()
    }
    fn validate(_: &Self::Configuration, credentials: &Self::Credentials) -> bool {
        !credentials.token.is_empty() && credentials.token.len() <= 16 * 1024
    }
    fn error_code(code: &str) -> &'static str {
        response::error_code(code)
    }
    fn limitations(values: &[String]) -> Vec<&'static str> {
        response::limitations(values)
    }
}
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
    permesh_native_runtime::serve::<Github, _, _, _, _, _>(
        reader,
        writer,
        |id, config, mut credentials| async move {
            let token = Secret::new(std::mem::take(&mut *credentials.token));
            factory(id, config.organizations, token)
        },
    )
    .await
}
#[cfg(test)]
mod request_tests;
#[cfg(test)]
mod tests;
