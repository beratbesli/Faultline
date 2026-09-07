use clap::Parser;
use faultline::cli::{Cli, Commands};
use faultline::config::FaultlineConfig;
use faultline::error::Result;
use std::path::Path;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let log_level = if cli.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .without_time()
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    match cli.command {
        Commands::Init(args) => {
            let config_path = Path::new("faultline.yaml");
            if config_path.exists() && !args.force {
                eprintln!("faultline.yaml already exists. Use --force to overwrite.");
                return Ok(());
            }

            let mut config = FaultlineConfig::default();
            config.database.db_type = args.db_type;
            if let Some(up) = args.up_sql {
                config.migration.up_sql = Some(up);
            }
            if let Some(down) = args.down_sql {
                config.migration.down_sql = Some(down);
            }

            let yaml = config.to_yaml()?;
            std::fs::write(config_path, yaml)?;
            println!("Initialized faultline.yaml successfully.");
            Ok(())
        }
        _ => {
            println!("Command recognized. Running in milestone foundation mode.");
            Ok(())
        }
    }
}
