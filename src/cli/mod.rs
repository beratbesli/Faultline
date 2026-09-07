use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "faultline",
    author = "Berat Besli <beratbesli26@gmail.com>",
    version = "0.1.0",
    about = "Faultline — Automatic database migration counterexample synthesizer",
    long_about = "Faultline generates schema-valid adversarial database states, executes the real migration, observes failures or information loss, and reduces discovered problems into minimal reproducible counterexamples."
)]
pub struct Cli {
    #[arg(
        short,
        long,
        global = true,
        help = "Path to faultline.yaml configuration file"
    )]
    pub config: Option<PathBuf>,

    #[arg(short, long, global = true, help = "Enable verbose debug logging")]
    pub verbose: bool,

    #[arg(
        long,
        global = true,
        help = "Output in structured machine-readable JSON format"
    )]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Initialize a new faultline.yaml in the current workspace")]
    Init(InitArgs),

    #[command(about = "Verify environment, database connectivity, and configuration health")]
    Doctor(DoctorArgs),

    #[command(about = "Inspect database schema and display discovered tables and constraints")]
    Inspect(InspectArgs),

    #[command(about = "Run counterexample search against database migrations")]
    Test(TestArgs),

    #[command(about = "Show status of current or previous search sessions")]
    Status(StatusArgs),

    #[command(about = "Resume an interrupted search session")]
    Resume(ResumeArgs),

    #[command(about = "Minimize a discovered counterexample to its smallest reproducing state")]
    Minimize(MinimizeArgs),

    #[command(about = "Replay and verify a counterexample reproducibly")]
    Replay(ReplayArgs),

    #[command(about = "Generate human-readable or structured JSON report of findings")]
    Report(ReportArgs),

    #[command(about = "List discovered counterexamples")]
    Counterexamples(CounterexamplesArgs),

    #[command(about = "Export a standalone reproduction package for bug reports and CI")]
    Export(ExportArgs),

    #[command(about = "Run Model Context Protocol (MCP) server over stdio for AI agents")]
    Mcp(McpArgs),
}

#[derive(Args, Debug)]
pub struct InitArgs {
    #[arg(
        short,
        long,
        default_value = "postgres",
        help = "Database type (postgres)"
    )]
    pub db_type: String,

    #[arg(short, long, help = "Path to up migration SQL file")]
    pub up_sql: Option<PathBuf>,

    #[arg(short, long, help = "Path to down migration SQL file")]
    pub down_sql: Option<PathBuf>,

    #[arg(short, long, help = "Force overwrite if faultline.yaml already exists")]
    pub force: bool,
}

#[derive(Args, Debug, Default)]
pub struct DoctorArgs {}

#[derive(Args, Debug, Default)]
pub struct InspectArgs {
    #[arg(short, long, help = "Inspect specific table only")]
    pub table: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct TestArgs {
    #[arg(
        short,
        long,
        help = "Random seed for deterministic search reproducibility"
    )]
    pub seed: Option<u64>,

    #[arg(short, long, help = "Maximum number of candidate states to test")]
    pub experiments: Option<usize>,

    #[arg(long, help = "Time limit in seconds")]
    pub time_limit: Option<u64>,

    #[arg(
        long,
        help = "Specific strategy to run (collision, precision, nullability, boundary, random)"
    )]
    pub strategy: Option<String>,

    #[arg(long, help = "Run round-trip UP -> DOWN -> COMPARE migration test")]
    pub roundtrip: bool,

    #[arg(
        long,
        help = "Skip automatic minimization of discovered counterexamples"
    )]
    pub no_minimize: bool,

    #[arg(long, help = "Override isolation safety checks (USE WITH CAUTION)")]
    pub allow_non_isolated: bool,
}

#[derive(Args, Debug, Default)]
pub struct StatusArgs {}

#[derive(Args, Debug, Default)]
pub struct ResumeArgs {
    #[arg(short, long, help = "Session ID to resume (defaults to latest)")]
    pub session_id: Option<String>,

    #[arg(short, long, help = "Additional experiment budget")]
    pub experiments: Option<usize>,
}

#[derive(Args, Debug)]
pub struct MinimizeArgs {
    #[arg(help = "Path or ID of counterexample to minimize")]
    pub target: String,
}

#[derive(Args, Debug)]
pub struct ReplayArgs {
    #[arg(help = "Path or ID of counterexample to replay")]
    pub target: String,

    #[arg(
        short,
        long,
        default_value = "1",
        help = "Number of times to replay for confidence check"
    )]
    pub repeat: usize,
}

#[derive(Args, Debug, Default)]
pub struct ReportArgs {
    #[arg(
        short,
        long,
        help = "Specific counterexample ID or session ID to report"
    )]
    pub id: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct CounterexamplesArgs {}

#[derive(Args, Debug)]
pub struct ExportArgs {
    #[arg(help = "Counterexample ID or path")]
    pub target: String,

    #[arg(help = "Output directory path for standalone reproduction package")]
    pub output_dir: PathBuf,
}

#[derive(Args, Debug, Default)]
pub struct McpArgs {}
