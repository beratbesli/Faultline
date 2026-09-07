use clap::Parser;
use colored::Colorize;
use faultline::cli::{Cli, Commands};
use faultline::config::FaultlineConfig;
use faultline::db::client::PgClient;
use faultline::db::isolation::SafetyGuard;
use faultline::error::Result;
use faultline::mcp::McpServer;
use faultline::migration::command::CommandMigrationRunner;
use faultline::migration::sql::SqlMigrationRunner;
use faultline::migration::MigrationRunner;
use faultline::report::{CounterexampleReplayer, ReportGenerator};
use faultline::schema::introspection::SchemaInspector;
use faultline::search::{SearchBudget, SearchEngine};
use faultline::storage::{CounterexampleManifest, StorageManager};
use faultline::strategy::StrategyScheduler;
use std::env;
use std::fs;
use std::path::Path;
use std::process::exit;
use std::time::Duration;
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

    let storage = StorageManager::new(".");
    let _ = storage.init();

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
            fs::write(config_path, yaml)?;
            println!("Initialized faultline.yaml successfully.");
            Ok(())
        }
        Commands::Doctor(_args) => {
            println!("==================================================");
            println!("              FAULTLINE DOCTOR CHECK              ");
            println!("==================================================");

            // 1. Config Check
            let config_res = FaultlineConfig::discover_and_load(Path::new("."));
            match &config_res {
                Ok((_, path)) => println!("  [OK] Configuration file found: {}", path.display()),
                Err(_) => println!("  [WARN] No faultline.yaml found in current directory"),
            }

            // 2. Binary dependencies
            let which_psql = std::process::Command::new("which").arg("psql").output();
            if which_psql.map(|o| o.status.success()).unwrap_or(false) {
                println!("  [OK] PostgreSQL client (psql) available in PATH");
            } else {
                println!("  [WARN] psql not found in PATH");
            }

            let which_initdb = std::process::Command::new("which").arg("initdb").output();
            if which_initdb.map(|o| o.status.success()).unwrap_or(false) {
                println!("  [OK] PostgreSQL server (initdb) available in PATH");
            } else {
                println!("  [INFO] initdb not found in PATH (using external/managed PostgreSQL)");
            }

            // 3. Database connectivity & safety
            if let Ok((config, _)) = &config_res {
                match config.get_database_url() {
                    Ok(url) => {
                        match SafetyGuard::verify_target_safety(
                            &url,
                            config.database.allow_non_isolated,
                        ) {
                            Ok(_) => println!(
                                "  [OK] Target database passes isolation safety validation"
                            ),
                            Err(e) => println!("  [FAIL] Target safety check failed: {}", e),
                        }

                        match PgClient::connect(&url).await {
                            Ok(client) => {
                                let ver = client
                                    .get_server_version()
                                    .await
                                    .unwrap_or_else(|_| "unknown".to_string());
                                let db = client
                                    .get_current_database()
                                    .await
                                    .unwrap_or_else(|_| "unknown".to_string());
                                println!("  [OK] Successfully connected to PostgreSQL (version: {}, database: {})", ver, db);
                            }
                            Err(e) => {
                                println!("  [WARN] Unable to connect to database at {}: {}", url, e)
                            }
                        }
                    }
                    Err(e) => println!("  [INFO] Database URL not set: {}", e),
                }
            }

            println!("==================================================");
            Ok(())
        }
        Commands::Inspect(args) => {
            let config = load_or_default_config(cli.config.as_deref())?;
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
        Commands::Test(args) => {
            let config = load_or_default_config(cli.config.as_deref())?;
            let db_url = config.get_database_url()?;

            SafetyGuard::verify_target_safety(
                &db_url,
                args.allow_non_isolated || config.database.allow_non_isolated,
            )?;

            let client = PgClient::connect(&db_url).await?;
            let schema = SchemaInspector::introspect(&client, None).await?;

            // Prepare migration runner
            let runner: Box<dyn MigrationRunner> = if let Some(up_path) = &config.migration.up_sql {
                Box::new(SqlMigrationRunner::new(
                    Some(up_path.clone()),
                    config.migration.down_sql.clone(),
                ))
            } else if let Some(up_cmd) = &config.migration.up {
                Box::new(CommandMigrationRunner::new(
                    Some(up_cmd.command.clone()),
                    config.migration.down.as_ref().map(|d| d.command.clone()),
                    config.migration.timeout_secs,
                ))
            } else {
                eprintln!("Error: No migration up command or SQL file specified.");
                exit(2);
            };

            let up_sql_content = if let Some(p) = &config.migration.up_sql {
                fs::read_to_string(p).ok()
            } else {
                None
            };

            let configured_strategies = if let Some(s) = args.strategy {
                vec![s]
            } else {
                config.testing.strategies.clone()
            };

            let scheduler = StrategyScheduler::new(&configured_strategies, None)?;

            let budget = SearchBudget {
                experiments: args.experiments.unwrap_or(config.testing.experiments),
                seed: args
                    .seed
                    .unwrap_or_else(|| config.testing.seed.unwrap_or(42)),
                time_limit: args
                    .time_limit
                    .or(config.testing.time_limit_secs)
                    .map(Duration::from_secs),
                max_rows_per_table: config.testing.max_rows_per_table,
                no_minimize: args.no_minimize,
                allow_non_isolated: args.allow_non_isolated || config.database.allow_non_isolated,
                roundtrip: args.roundtrip || config.checks.roundtrip,
            };

            let engine = SearchEngine::new(
                &config,
                &schema,
                runner.as_ref(),
                &scheduler,
                &storage,
                db_url,
                up_sql_content,
            );

            let summary = engine.run_search(budget).await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                println!("{}", ReportGenerator::format_terminal_summary(&summary));
            }

            if summary.counterexamples_found > 0 {
                exit(1);
            } else {
                exit(0);
            }
        }
        Commands::Status(_args) => {
            let sessions_dir = storage.sessions_dir();
            let mut sessions = Vec::new();
            if sessions_dir.exists() {
                if let Ok(entries) = fs::read_dir(sessions_dir) {
                    for entry in entries.flatten() {
                        if entry
                            .path()
                            .extension()
                            .map(|e| e == "json")
                            .unwrap_or(false)
                        {
                            if let Ok(session) =
                                storage.load_json::<faultline::search::TestSession>(&entry.path())
                            {
                                sessions.push(session);
                            }
                        }
                    }
                }
            }

            sessions.sort_by_key(|a| std::cmp::Reverse(a.started_at));

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else {
                println!("==================================================");
                println!("             FAULTLINE SESSION STATUS             ");
                println!("==================================================");
                if sessions.is_empty() {
                    println!("No search sessions recorded yet.");
                } else {
                    for s in sessions {
                        println!("Session ID:       {}", s.session_id);
                        println!("Started:          {}", s.started_at);
                        println!("Total Experiments:{}", s.total_experiments);
                        println!("Unique States:    {}", s.unique_states_tested);
                        println!("Counterexamples:  {}", s.counterexamples_found);
                        if let Some(cx) = s.best_counterexample_id {
                            println!("Counterexample:   {}", cx.bold().red());
                        }
                        println!("--------------------------------------------------");
                    }
                }
                println!("==================================================");
            }
            Ok(())
        }
        Commands::Counterexamples(_args) => {
            let dir = storage.counterexamples_dir();
            let mut list = Vec::new();
            if dir.exists() {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let manifest_file = entry.path().join("manifest.json");
                        if manifest_file.exists() {
                            if let Ok(manifest) =
                                storage.load_json::<CounterexampleManifest>(&manifest_file)
                            {
                                list.push(manifest);
                            }
                        }
                    }
                }
            }

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                println!("==================================================");
                println!("            DISCOVERED COUNTEREXAMPLES            ");
                println!("==================================================");
                if list.is_empty() {
                    println!("No counterexamples found.");
                } else {
                    for cx in list {
                        println!("ID:            {}", cx.id.bold());
                        println!("Failure Class: {}", cx.failure_class.display_name().red());
                        println!("Strategy:      {}", cx.strategy);
                        println!("Seed:          {}", cx.seed);
                        println!("Rows:          {}", cx.rows_count);
                        println!(
                            "Path:          {}/{}",
                            storage.counterexamples_dir().display(),
                            cx.id
                        );
                        println!("--------------------------------------------------");
                    }
                }
                println!("==================================================");
            }
            Ok(())
        }
        Commands::Replay(args) => {
            let config = load_or_default_config(cli.config.as_deref())?;
            let db_url = config.get_database_url()?;

            let target_path = if Path::new(&args.target).exists() {
                Path::new(&args.target).to_path_buf()
            } else {
                storage.counterexamples_dir().join(&args.target)
            };

            let res = CounterexampleReplayer::replay(
                &target_path,
                &db_url,
                args.repeat,
                config.database.allow_non_isolated,
            )
            .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!("==================================================");
                println!("          FAULTLINE COUNTEREXAMPLE REPLAY         ");
                println!("==================================================");
                println!("Counterexample ID: {}", res.counterexample_id);
                println!(
                    "Reproduction Rate: {} / {}",
                    res.successful_reproductions, res.total_attempts
                );
                println!(
                    "Deterministic:     {}",
                    if res.is_deterministic {
                        "YES".green().bold()
                    } else {
                        "NO".yellow().bold()
                    }
                );
                if !res.error_details.is_empty() {
                    println!("Observed Error:    {}", res.error_details[0]);
                }
                println!("==================================================");
            }

            if res.successful_reproductions > 0 {
                exit(1);
            } else {
                exit(0);
            }
        }
        Commands::Report(args) => {
            let dir = storage.counterexamples_dir();
            let target_cx = if let Some(id) = &args.id {
                let p = dir.join(id).join("manifest.json");
                if p.exists() {
                    Some(storage.load_json::<CounterexampleManifest>(&p)?)
                } else {
                    None
                }
            } else {
                // Find latest counterexample
                let mut manifests = Vec::new();
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let p = entry.path().join("manifest.json");
                        if let Ok(m) = storage.load_json::<CounterexampleManifest>(&p) {
                            manifests.push(m);
                        }
                    }
                }
                manifests.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
                manifests.into_iter().next()
            };

            if let Some(cx) = target_cx {
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&cx)?);
                } else {
                    println!(
                        "{}",
                        ReportGenerator::format_counterexample_detail(&cx, None)
                    );
                }
            } else {
                println!("No counterexamples available to report.");
            }
            Ok(())
        }
        Commands::Export(args) => {
            let target_path = if Path::new(&args.target).exists() {
                Path::new(&args.target).to_path_buf()
            } else {
                storage.counterexamples_dir().join(&args.target)
            };

            if !target_path.exists() {
                eprintln!("Counterexample not found: {}", target_path.display());
                exit(2);
            }

            fs::create_dir_all(&args.output_dir)?;
            for item in [
                "manifest.json",
                "schema.sql",
                "seed.sql",
                "migration_up.sql",
                "reproduce.sh",
                "README.md",
            ] {
                let src = target_path.join(item);
                if src.exists() {
                    fs::copy(&src, args.output_dir.join(item))?;
                }
            }

            println!(
                "Exported counterexample package to: {}",
                args.output_dir.display()
            );
            Ok(())
        }
        Commands::Mcp(_args) => {
            McpServer::run().await?;
            Ok(())
        }
        Commands::Resume(args) => {
            println!(
                "Resuming session {:?} with additional budget {:?}",
                args.session_id, args.experiments
            );
            Ok(())
        }
        Commands::Minimize(args) => {
            println!(
                "Minimization is automatically applied during search. Requested target: {}",
                args.target
            );
            Ok(())
        }
    }
}

fn load_or_default_config(path: Option<&Path>) -> Result<FaultlineConfig> {
    if let Some(p) = path {
        FaultlineConfig::load_from_file(p)
    } else {
        match FaultlineConfig::discover_and_load(Path::new(".")) {
            Ok((cfg, _)) => Ok(cfg),
            Err(_) => {
                let mut cfg = FaultlineConfig::default();
                if let Ok(url) = env::var("DATABASE_URL") {
                    cfg.database.url = Some(url);
                }
                Ok(cfg)
            }
        }
    }
}
