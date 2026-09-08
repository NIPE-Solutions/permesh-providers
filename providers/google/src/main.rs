// SPDX-License-Identifier: MIT
fn main() -> std::process::ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return std::process::ExitCode::from(1),
    };
    let result = runtime.block_on(permesh_provider_google::protocol::run(
        tokio::io::stdin(),
        tokio::io::stdout(),
    ));
    // Tokio's standard-I/O workers cannot interrupt a blocked OS read/write.
    // Bound shutdown so a host retaining stdin cannot keep this process alive
    // after its request deadline. Process exit ends remaining worker threads.
    runtime.shutdown_timeout(std::time::Duration::from_millis(100));
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::from(1),
    }
}
