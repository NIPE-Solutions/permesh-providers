// SPDX-License-Identifier: MIT
//! Explicit host-provided transport policy; never inherit process proxy settings.
use permesh_provider_sdk::{ProviderError, network::NetworkContext};
use reqwest::{Certificate, Client, ClientBuilder, NoProxy, Proxy};

fn invalid() -> ProviderError {
    ProviderError::new(
        "configuration",
        "Invalid provider network context or HTTP client settings",
    )
}
pub fn client_builder(network: Option<&NetworkContext>) -> Result<ClientBuilder, ProviderError> {
    let mut builder = Client::builder().no_proxy();
    if let Some(network) = network {
        network.validate().map_err(|_| invalid())?;
        if let Some(proxy) = &network.https_proxy {
            let proxy = Proxy::https(proxy)
                .map_err(|_| invalid())?
                .no_proxy(NoProxy::from_string(&network.no_proxy.join(",")));
            builder = builder.proxy(proxy);
        }
        if let Some(bundle) = &network.ca_bundle_pem {
            let certificates =
                Certificate::from_pem_bundle(bundle.as_bytes()).map_err(|_| invalid())?;
            if certificates.is_empty() {
                return Err(invalid());
            }
            // Force parsing of every DER root without depending on platform trust-store
            // parsing behavior. This validation-only client never makes a request.
            Client::builder()
                .no_proxy()
                .tls_certs_only(certificates.clone())
                .build()
                .map_err(|_| invalid())?;
            for certificate in certificates {
                builder = builder.add_root_certificate(certificate);
            }
        }
    }
    Ok(builder)
}

/// Validate all transport material before an adapter can consume credentials.
pub(crate) fn validate(network: &NetworkContext) -> Result<(), ProviderError> {
    client_builder(Some(network)).map(|_| ())
}
