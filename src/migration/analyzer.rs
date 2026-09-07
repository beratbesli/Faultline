use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MigrationHints {
    pub suggested_strategies: Vec<String>,
    pub detected_operations: Vec<String>,
    pub target_columns: Vec<String>,
}

pub struct MigrationAnalyzer;

impl MigrationAnalyzer {
    pub fn analyze_sql(sql: &str) -> MigrationHints {
        let mut hints = MigrationHints::default();
        let upper_sql = sql.to_uppercase();

        // 1. Check for case-folding / string transformations
        if upper_sql.contains("LOWER(") || upper_sql.contains("UPPER(") {
            hints
                .detected_operations
                .push("CASE_NORMALIZATION".to_string());
            if !hints
                .suggested_strategies
                .contains(&"collision".to_string())
            {
                hints.suggested_strategies.push("collision".to_string());
            }
        }

        if upper_sql.contains("TRIM(") || upper_sql.contains("BTRIM(") {
            hints
                .detected_operations
                .push("WHITESPACE_TRIM".to_string());
            if !hints
                .suggested_strategies
                .contains(&"collision".to_string())
            {
                hints.suggested_strategies.push("collision".to_string());
            }
        }

        // 2. Check for unique constraints or indexes
        if upper_sql.contains("UNIQUE") {
            hints
                .detected_operations
                .push("UNIQUE_CONSTRAINT".to_string());
            if !hints
                .suggested_strategies
                .contains(&"collision".to_string())
            {
                hints.suggested_strategies.push("collision".to_string());
            }
        }

        // 3. Check for numeric casts / type changes
        if upper_sql.contains("CAST(")
            || upper_sql.contains("::INTEGER")
            || upper_sql.contains("::INT")
            || (upper_sql.contains("ALTER")
                && upper_sql.contains("TYPE")
                && upper_sql.contains("INT"))
            || upper_sql.contains("ROUND(")
        {
            hints
                .detected_operations
                .push("NUMERIC_TRANSFORMATION".to_string());
            if !hints
                .suggested_strategies
                .contains(&"precision".to_string())
            {
                hints.suggested_strategies.push("precision".to_string());
            }
        }

        // 4. Check for NOT NULL introduction
        if upper_sql.contains("SET NOT NULL")
            || (upper_sql.contains("ADD COLUMN") && upper_sql.contains("NOT NULL"))
        {
            hints
                .detected_operations
                .push("NOT_NULL_ADDITION".to_string());
            if !hints
                .suggested_strategies
                .contains(&"nullability".to_string())
            {
                hints.suggested_strategies.push("nullability".to_string());
            }
        }

        // 5. Check for length constraints
        if upper_sql.contains("VARCHAR(") || upper_sql.contains("CHAR(") {
            hints
                .detected_operations
                .push("LENGTH_CONSTRAINT".to_string());
            if !hints.suggested_strategies.contains(&"boundary".to_string()) {
                hints.suggested_strategies.push("boundary".to_string());
            }
        }

        // Always include random fuzzer and boundary as baseline
        if !hints.suggested_strategies.contains(&"boundary".to_string()) {
            hints.suggested_strategies.push("boundary".to_string());
        }
        if !hints.suggested_strategies.contains(&"random".to_string()) {
            hints.suggested_strategies.push("random".to_string());
        }

        hints
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_case_collision() {
        let sql = "UPDATE users SET normalized_email = LOWER(email); CREATE UNIQUE INDEX u_idx ON users(normalized_email);";
        let hints = MigrationAnalyzer::analyze_sql(sql);
        assert!(hints
            .suggested_strategies
            .contains(&"collision".to_string()));
        assert!(hints
            .detected_operations
            .contains(&"CASE_NORMALIZATION".to_string()));
        assert!(hints
            .detected_operations
            .contains(&"UNIQUE_CONSTRAINT".to_string()));
    }

    #[test]
    fn test_analyze_precision_loss() {
        let sql = "ALTER TABLE products ALTER COLUMN price TYPE INTEGER USING ROUND(price);";
        let hints = MigrationAnalyzer::analyze_sql(sql);
        assert!(hints
            .suggested_strategies
            .contains(&"precision".to_string()));
        assert!(hints
            .detected_operations
            .contains(&"NUMERIC_TRANSFORMATION".to_string()));
    }
}
