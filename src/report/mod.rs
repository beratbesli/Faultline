pub mod replay;

pub use replay::{CounterexampleReplayer, ReplayResult};

use crate::search::SearchSummary;
use crate::storage::CounterexampleManifest;
use colored::Colorize;

pub struct ReportGenerator;

impl ReportGenerator {
    pub fn format_terminal_summary(summary: &SearchSummary) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "\n{}\n",
            "==================================================".bold()
        ));
        out.push_str(&format!(
            "                {}\n",
            "FAULTLINE REPORT".bold().cyan()
        ));
        out.push_str(&format!(
            "{}\n",
            "==================================================".bold()
        ));
        out.push_str(&format!(
            "Session ID:              {}\n",
            summary.session_id
        ));
        out.push_str(&format!(
            "Duration:                {:.2}s\n",
            summary.duration_ms as f64 / 1000.0
        ));
        out.push_str(&format!(
            "Experiments Executed:    {}\n",
            summary.experiments_executed
        ));
        out.push_str(&format!(
            "Unique States Tested:    {}\n",
            summary.unique_states_tested
        ));
        out.push_str(&format!(
            "Counterexamples Found:   {}\n",
            summary.counterexamples_found
        ));
        out.push_str(&format!(
            "{}\n",
            "--------------------------------------------------"
        ));

        if let Some(cx) = &summary.best_counterexample {
            out.push_str(&format!("{}\n", "COUNTEREXAMPLE FOUND".bold().red()));
            out.push_str(&format!("ID:                      {}\n", cx.id));
            out.push_str(&format!(
                "Failure Class:           {}\n",
                cx.failure_class.display_name().bold()
            ));
            out.push_str(&format!("Discovery Strategy:      {}\n", cx.strategy));
            out.push_str(&format!("Seed:                    {}\n", cx.seed));
            out.push_str(&format!("Minimal Reproducing Rows:{}\n", cx.rows_count));
            out.push_str(&format!("Error:\n  {}\n", cx.error_message));

            if let Some(min_state) = &summary.minimal_state {
                out.push_str(&format!(
                    "\n{}\n",
                    "Minimal Reproducing State:".bold().yellow()
                ));
                for (tbl, tdata) in &min_state.tables {
                    out.push_str(&format!("  Table: {}\n", tbl.bold()));
                    for row in &tdata.rows {
                        let pairs: Vec<String> = row
                            .values
                            .iter()
                            .map(|(k, v)| format!("{}={}", k, v.to_sql_literal()))
                            .collect();
                        out.push_str(&format!("    - {}\n", pairs.join(", ")));
                    }
                }
            }
        } else {
            out.push_str(&format!("{}\n", "NO COUNTEREXAMPLE FOUND".green().bold()));
            out.push_str(
                "No breaking database state discovered within configured experiment budget.\n",
            );
        }
        out.push_str(&format!(
            "{}\n",
            "==================================================".bold()
        ));

        out
    }

    pub fn format_counterexample_detail(
        manifest: &CounterexampleManifest,
        replay: Option<&ReplayResult>,
    ) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "\n{}\n",
            "==================================================".bold()
        ));
        out.push_str(&format!(
            "             {}\n",
            "COUNTEREXAMPLE DETAIL".bold().cyan()
        ));
        out.push_str(&format!(
            "{}\n",
            "==================================================".bold()
        ));
        out.push_str(&format!("ID:                      {}\n", manifest.id));
        out.push_str(&format!(
            "Failure Class:           {}\n",
            manifest.failure_class.display_name().bold()
        ));
        out.push_str(&format!("Strategy:                {}\n", manifest.strategy));
        out.push_str(&format!("Seed:                    {}\n", manifest.seed));
        out.push_str(&format!(
            "Rows Required:           {}\n",
            manifest.rows_count
        ));
        out.push_str(&format!("Error:\n  {}\n", manifest.error_message));

        if let Some(r) = replay {
            out.push_str(&format!(
                "Reproduction Confidence: {} / {} (Deterministic: {})\n",
                r.successful_reproductions, r.total_attempts, r.is_deterministic
            ));
        }
        out.push_str(&format!(
            "{}\n",
            "==================================================".bold()
        ));
        out
    }
}
