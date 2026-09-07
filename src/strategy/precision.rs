use crate::error::Result;
use crate::generator::relation::StateGenerator;
use crate::generator::{DatabaseState, SqlValue};
use crate::schema::{DataType, DatabaseSchema};
use crate::strategy::SearchStrategy;
use rand::Rng;
use rand_chacha::ChaCha8Rng;

pub struct PrecisionStrategy;

impl SearchStrategy for PrecisionStrategy {
    fn name(&self) -> &'static str {
        "precision"
    }

    fn generate_candidate(
        &self,
        schema: &DatabaseSchema,
        target_rows: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<DatabaseState> {
        let mut state = StateGenerator::generate_valid_state(schema, target_rows.max(1), rng)?;

        for table in &schema.tables {
            let num_cols: Vec<String> = table
                .columns
                .iter()
                .filter(|c| !table.is_pk_column(&c.name))
                .filter(|c| {
                    matches!(
                        c.data_type,
                        DataType::Numeric { .. } | DataType::Real | DataType::DoublePrecision
                    )
                })
                .map(|c| c.name.clone())
                .collect();

            if num_cols.is_empty() {
                continue;
            }

            if let Some(table_data) = state.tables.get_mut(&table.name) {
                let precision_samples = ["19.99", "0.99", "12.345", "100.50", "999999.99"];

                for (idx, row) in table_data.rows.iter_mut().enumerate() {
                    for col_name in &num_cols {
                        let sample = precision_samples[(idx
                            + rng.gen_range(0..precision_samples.len()))
                            % precision_samples.len()];
                        row.values
                            .insert(col_name.clone(), SqlValue::Numeric(sample.to_string()));
                    }
                }
            }
        }

        Ok(state)
    }
}
