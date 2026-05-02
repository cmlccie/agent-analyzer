use crate::cli::args::{OutputFormat, StatusArgs};
use crate::core::client::ApiClient;
use crate::core::target::{Status, Target, TargetKind};

pub async fn run(args: StatusArgs) -> anyhow::Result<()> {
    let client = ApiClient::new(args.url)?;

    let targets = client.state().await?;

    match args.format {
        OutputFormat::Json => print_json(&targets)?,
        OutputFormat::Text => print_text(&targets),
    }

    Ok(())
}

fn print_json(targets: &[Target]) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(targets)?);
    Ok(())
}

fn print_text(targets: &[Target]) {
    for kind in [
        TargetKind::Model,
        TargetKind::Agent,
        TargetKind::Tool,
        TargetKind::Website,
    ] {
        let section: Vec<&Target> = targets.iter().filter(|t| t.kind == kind).collect();
        if section.is_empty() {
            continue;
        }
        println!("\n{kind}s:");
        for t in section {
            let icon = if t.status.is_ok() { "✓" } else { "✗" };
            let source = t.source.to_string();
            let checked = t
                .status
                .checked_at()
                .map(|ts| ts.format("%H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "never".to_string());
            println!("  {icon}  {:<20} ({source})  [{checked}]", t.name);
            if let Status::Failed { error, .. } = &t.status {
                println!("     └─ {error}");
            }
        }
    }
    println!();
}
