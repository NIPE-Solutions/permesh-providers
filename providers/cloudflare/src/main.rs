// SPDX-License-Identifier: MIT OR Apache-2.0
fn main() -> std::process::ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return std::process::ExitCode::from(1),
    };
    let result = runtime.block_on(permesh_provider_cloudflare::protocol::run(
        tokio::io::stdin(),
        tokio::io::stdout(),
    ));
    // Bound shutdown when the host keeps its standard-input pipe open.
    runtime.shutdown_timeout(std::time::Duration::from_millis(100));
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::from(1),
    }
}
