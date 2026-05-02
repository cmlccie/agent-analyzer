use agent_analyzer::cli;

#[tokio::main]
async fn main() {
    if let Err(err) = cli::run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
