#[tokio::main]
async fn main() -> std::process::ExitCode {
    let arguments: Result<Vec<String>, _> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.into_string())
        .collect();
    let Ok(arguments) = arguments else {
        eprintln!("Usage Lens: invalid_argument");
        return std::process::ExitCode::FAILURE;
    };
    let code = usage_lens::cli::run_main(
        &arguments,
        tokio::io::stdin(),
        tokio::io::stdout(),
        tokio::io::stderr(),
    )
    .await;
    std::process::ExitCode::from(code as u8)
}
