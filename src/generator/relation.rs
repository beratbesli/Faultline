use crate::error::Result;
use crate::generator::primitive::PrimitiveGenerator;
use crate::generator::{DatabaseState, RowData, SqlValue, TableData};
use crate::schema::{DataType, DatabaseSchema, Table};
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use std::collections::{HashMap, HashSet};

pub struct StateGenerator;

impl StateGenerator {
    pub fn generate_valid_state(
        schema: &DatabaseSchema,
        rows_per_table: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<DatabaseState> {
        let mut state = DatabaseState::new();
        let table_order = schema.topological_order()?;

        // Keep track of primary key values generated per table to satisfy foreign keys
        // table_name -> map of (pk_column_name -> Vec<SqlValue>)
        let mut table_pks: HashMap<String, Vec<SqlValue>> = HashMap::new();

        for table_name in table_order {
            let table = match schema.get_table(&table_name) {
                Some(t) => t,
                None => continue,
            };

            let table_data = Self::generate_table_data(table, rows_per_table, &table_pks, rng)?;

            // Record PKs generated for child FK satisfaction
            if let Some(pk) = &table.primary_key {
                if let Some(pk_col) = pk.columns.first() {
                    let mut pks = Vec::new();
                    for row in &table_data.rows {
                        if let Some(val) = row.values.get(pk_col) {
                            pks.push(val.clone());
                        }
                    }
                    table_pks.insert(table_name.clone(), pks);
                }
            }

            state.tables.insert(table_name, table_data);
        }

        Ok(state)
    }

    fn generate_table_data(
        table: &Table,
        target_rows: usize,
        parent_pks: &HashMap<String, Vec<SqlValue>>,
        rng: &mut ChaCha8Rng,
    ) -> Result<TableData> {
        let mut rows = Vec::new();
        let mut seen_unique_values: HashMap<String, HashSet<String>> = HashMap::new();

        for i in 0..target_rows {
            let mut row_values = HashMap::new();

            for col in &table.columns {
                if col.is_generated {
                    continue;
                }

                // 1. Check if column is part of a foreign key
                let mut fk_value = None;
                for fk in &table.foreign_keys {
                    if let Some(_pos) = fk.columns.iter().position(|c| c == &col.name) {
                        if let Some(parents) = parent_pks.get(&fk.foreign_table) {
                            if !parents.is_empty() {
                                let idx = rng.gen_range(0..parents.len());
                                fk_value = Some(parents[idx].clone());
                                break;
                            }
                        }
                    }
                }

                if let Some(val) = fk_value {
                    row_values.insert(col.name.clone(), val);
                    continue;
                }

                // 2. Check if column is primary key
                if table.is_pk_column(&col.name) {
                    let pk_val = match &col.data_type {
                        DataType::SmallInt => SqlValue::SmallInt((i + 1) as i16),
                        DataType::Integer => SqlValue::Integer((i + 1) as i32),
                        DataType::BigInt => SqlValue::BigInt((i + 1) as i64),
                        DataType::Uuid => {
                            let u = uuid::Uuid::from_u128(rng.gen());
                            SqlValue::Uuid(u.to_string())
                        }
                        _ => SqlValue::Integer((i + 1) as i32),
                    };
                    row_values.insert(col.name.clone(), pk_val);
                    continue;
                }

                // 3. Otherwise generate value respecting uniqueness and nullability
                let mut val =
                    PrimitiveGenerator::generate_value(&col.data_type, col.is_nullable, true, rng);

                // If column has unique constraint, make sure pre-migration state is valid
                if table.is_unique_column(&col.name) {
                    let seen = seen_unique_values.entry(col.name.clone()).or_default();
                    let mut attempts = 0;
                    while seen.contains(&val.to_sql_literal()) && attempts < 20 {
                        val = match &col.data_type {
                            DataType::Integer => SqlValue::Integer(rng.gen_range(1000..999_999)),
                            DataType::Text | DataType::Varchar(_) => SqlValue::Text(format!(
                                "unique_{}_{}@domain.com",
                                i,
                                rng.gen_range(100..999)
                            )),
                            _ => PrimitiveGenerator::generate_value(
                                &col.data_type,
                                false,
                                false,
                                rng,
                            ),
                        };
                        attempts += 1;
                    }
                    seen.insert(val.to_sql_literal());
                }

                row_values.insert(col.name.clone(), val);
            }

            rows.push(RowData { values: row_values });
        }

        Ok(TableData {
            table_name: table.name.clone(),
            rows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::seed::GenerationSeed;
    use crate::schema::{Column, PrimaryKey};

    #[test]
    fn test_generate_valid_state_deterministic() {
        let table = Table {
            name: "users".to_string(),
            schema_name: "public".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: DataType::Integer,
                    is_nullable: false,
                    default_value: None,
                    is_identity: true,
                    is_generated: false,
                },
                Column {
                    name: "email".to_string(),
                    data_type: DataType::Text,
                    is_nullable: false,
                    default_value: None,
                    is_identity: false,
                    is_generated: false,
                },
            ],
            primary_key: Some(PrimaryKey {
                name: "pk_users".to_string(),
                columns: vec!["id".to_string()],
            }),
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
        };

        let schema = DatabaseSchema {
            tables: vec![table],
            enums: vec![],
        };

        let mut rng1 = GenerationSeed::new(12345).to_rng();
        let state1 = StateGenerator::generate_valid_state(&schema, 5, &mut rng1).unwrap();

        let mut rng2 = GenerationSeed::new(12345).to_rng();
        let state2 = StateGenerator::generate_valid_state(&schema, 5, &mut rng2).unwrap();

        assert_eq!(state1.total_rows(), 5);
        assert_eq!(state1.fingerprint(), state2.fingerprint());
    }
}
