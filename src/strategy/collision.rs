use crate::error::Result;
use crate::generator::relation::StateGenerator;
use crate::generator::{DatabaseState, SqlValue};
use crate::schema::{DataType, DatabaseSchema};
use crate::strategy::SearchStrategy;
use rand::Rng;
use rand_chacha::ChaCha8Rng;

pub struct CollisionStrategy;

impl SearchStrategy for CollisionStrategy {
    fn name(&self) -> &'static str {
        "collision"
    }

    fn generate_candidate(
        &self,
        schema: &DatabaseSchema,
        target_rows: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<DatabaseState> {
        let rows = target_rows.max(2);
        let mut state = StateGenerator::generate_valid_state(schema, rows, rng)?;

        // Find text columns in tables and introduce deliberate case-variation or whitespace collisions
        for table in &schema.tables {
            let text_cols: Vec<String> = table
                .columns
                .iter()
                .filter(|c| !table.is_pk_column(&c.name))
                .filter(|c| {
                    matches!(
                        c.data_type,
                        DataType::Text | DataType::Varchar(_) | DataType::Char(_)
                    )
                })
                .map(|c| c.name.clone())
                .collect();

            if text_cols.is_empty() {
                continue;
            }

            if let Some(table_data) = state.tables.get_mut(&table.name) {
                if table_data.rows.len() >= 2 {
                    let col_name = &text_cols[rng.gen_range(0..text_cols.len())];

                    let collision_pairs = [
                        ("Berat@example.com", "berat@example.com"),
                        ("Alice@domain.org", "alice@domain.org"),
                        ("Charlie@company.net", "charlie@company.net"),
                        (" ADMIN@system.io ", "admin@system.io"),
                        ("User123", "user123"),
                    ];

                    let pair_idx = rng.gen_range(0..collision_pairs.len());
                    let (val1, val2) = collision_pairs[pair_idx];

                    table_data.rows[0]
                        .values
                        .insert(col_name.clone(), SqlValue::Text(val1.to_string()));
                    table_data.rows[1]
                        .values
                        .insert(col_name.clone(), SqlValue::Text(val2.to_string()));
                }
            }
        }

        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::seed::GenerationSeed;
    use crate::schema::{Column, PrimaryKey, Table};

    #[test]
    fn test_collision_strategy_injects_case_pairs() {
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

        let mut rng = GenerationSeed::new(999).to_rng();
        let strategy = CollisionStrategy;
        let state = strategy.generate_candidate(&schema, 2, &mut rng).unwrap();

        let users = &state.tables["users"].rows;
        let email1 = &users[0].values["email"];
        let email2 = &users[1].values["email"];

        match (email1, email2) {
            (SqlValue::Text(s1), SqlValue::Text(s2)) => {
                // Must be different in pre-migration state
                assert_ne!(s1, s2);
                // But identical after lowering or trimming
                assert_eq!(s1.trim().to_lowercase(), s2.trim().to_lowercase());
            }
            _ => panic!("Expected text values"),
        }
    }
}
