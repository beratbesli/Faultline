use crate::error::Result;
use crate::generator::relation::StateGenerator;
use crate::generator::{DatabaseState, SqlValue};
use crate::schema::{DataType, DatabaseSchema};
use crate::strategy::SearchStrategy;
use rand_chacha::ChaCha8Rng;

pub struct BoundaryStrategy;

impl SearchStrategy for BoundaryStrategy {
    fn name(&self) -> &'static str {
        "boundary"
    }

    fn generate_candidate(
        &self,
        schema: &DatabaseSchema,
        target_rows: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<DatabaseState> {
        let mut state = StateGenerator::generate_valid_state(schema, target_rows.max(2), rng)?;

        for table in &schema.tables {
            if let Some(table_data) = state.tables.get_mut(&table.name) {
                for (row_idx, row) in table_data.rows.iter_mut().enumerate() {
                    for col in &table.columns {
                        if table.is_pk_column(&col.name) || col.is_generated {
                            continue;
                        }

                        let b_val = match &col.data_type {
                            DataType::SmallInt => {
                                if row_idx % 2 == 0 {
                                    SqlValue::SmallInt(i16::MAX)
                                } else {
                                    SqlValue::SmallInt(i16::MIN)
                                }
                            }
                            DataType::Integer => {
                                if row_idx % 2 == 0 {
                                    SqlValue::Integer(i32::MAX)
                                } else {
                                    SqlValue::Integer(i32::MIN)
                                }
                            }
                            DataType::BigInt => {
                                if row_idx % 2 == 0 {
                                    SqlValue::BigInt(i64::MAX)
                                } else {
                                    SqlValue::BigInt(i64::MIN)
                                }
                            }
                            DataType::Text | DataType::Varchar(_) | DataType::Char(_) => {
                                if row_idx % 3 == 0 {
                                    SqlValue::Text("".to_string())
                                } else if row_idx % 3 == 1 {
                                    SqlValue::Text("a".to_string())
                                } else {
                                    SqlValue::Text("x".repeat(250))
                                }
                            }
                            _ => continue,
                        };

                        row.values.insert(col.name.clone(), b_val);
                    }
                }
            }
        }

        Ok(state)
    }
}
