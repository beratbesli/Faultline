use clap::Parser;
use faultline::cli::{Cli, Commands};
use faultline::config::FaultlineConfig;
use faultline::db::PgClient;
use faultline::error::Result;
use faultline::schema::introspection::SchemaInspector;
use std::env;
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
        Commands::Inspect(args) => {
            let config = match cli.config.as_ref() {
                Some(p) => FaultlineConfig::load_from_file(p)?,
                None => match FaultlineConfig::discover_and_load(Path::new(".")) {
                    Ok((cfg, _)) => cfg,
                    Err(_) => {
                        let mut cfg = FaultlineConfig::default();
                        if let Ok(url) = env::var("DATABASE_URL") {
                            cfg.database.url = Some(url);
                        }
                        cfg
                    }
                },
            };

            let db_url = config.get_database_url()?;
            let client = PgClient::connect(&db_url).await?;
            let schema = SchemaInspector::introspect(&client, args.table.as_deref()).await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&schema)?);
            } else {
                println!("==================================================");
                println!("           FAULTLINE SCHEMA INSPECTION            ");
                println!("==================================================");
                println!("Schema Fingerprint: {}", schema.fingerprint());
                println!("Discovered Tables:  {}", schema.tables.len());
                println!("Discovered Enums:   {}", schema.enums.len());
                println!("--------------------------------------------------");

                for table in &schema.tables {
                    println!("\nTable: {}", table.name);
                    println!("Columns:");
                    for col in &table.columns {
                        let nullable_str = if col.is_nullable { "NULL" } else { "NOT NULL" };
                        let pk_str = if table.is_pk_column(&col.name) {
                            " [PK]"
                        } else {
                            ""
                        };
                        let uq_str = if table.is_unique_column(&col.name)
                            && !table.is_pk_column(&col.name)
                        {
                            " [UNIQUE]"
                        } else {
                            ""
                        };
                        println!(
                            "  - {:<20} {:<15} {}{}{}",
                            col.name,
                            col.data_type.display_name(),
                            nullable_str,
                            pk_str,
                            uq_str
                        );
                    }

                    if !table.foreign_keys.is_empty() {
                        println!("Foreign Keys:");
                        for fk in &table.foreign_keys {
                            println!(
                                "  - ({}) -> {}({}) [ON DELETE {}, ON UPDATE {}]",
                                fk.columns.join(", "),
                                fk.foreign_table,
                                fk.foreign_columns.join(", "),
                                fk.on_delete,
                                fk.on_update
                            );
                        }
                    }

                    if !table.check_constraints.is_empty() {
                        println!("Check Constraints:");
                        for ck in &table.check_constraints {
                            println!("  - {}: {}", ck.name, ck.clause);
                        }
                    }
                }
                println!("\n==================================================");
            }
            Ok(())
        }
        _ => {
            println!("Command will be handled in corresponding milestone.");
            Ok(())
        }
    }
}
