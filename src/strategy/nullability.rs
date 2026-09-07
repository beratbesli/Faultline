use crate::error::Result;
use crate::generator::relation::StateGenerator;
use crate::generator::{DatabaseState, SqlValue};
use crate::schema::DatabaseSchema;
use crate::strategy::SearchStrategy;
use rand_chacha::ChaCha8Rng;

pub struct NullabilityStrategy;

impl SearchStrategy for NullabilityStrategy {
    fn name(&self) -> &'static str {
        "nullability"
    }

    fn generate_candidate(
        &self,
        schema: &DatabaseSchema,
        target_rows: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<DatabaseState> {
        let mut state = StateGenerator::generate_valid_state(schema, target_rows.max(1), rng)?;

        for table in &schema.tables {
            let nullable_cols: Vec<String> = table
                .columns
                .iter()
                .filter(|c| c.is_nullable && !table.is_pk_column(&c.name))
                .map(|c| c.name.clone())
                .collect();

            if nullable_cols.is_empty() {
                continue;
            }

            if let Some(table_data) = state.tables.get_mut(&table.name) {
                // Ensure at least one row has NULL in each nullable column
                if let Some(first_row) = table_data.rows.first_mut() {
                    for col_name in &nullable_cols {
                        first_row.values.insert(col_name.clone(), SqlValue::Null);
                    }
                }
            }
        }

        Ok(state)
    }
}
