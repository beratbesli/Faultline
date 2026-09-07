use crate::generator::{DatabaseState, SqlValue};
use crate::minimizer::TestFn;
use crate::schema::DatabaseSchema;
use std::collections::{HashMap, HashSet};

pub async fn reduce_rows<'a>(
    schema: &DatabaseSchema,
    initial_state: &DatabaseState,
    test_fn: &TestFn<'a>,
) -> DatabaseState {
    let mut current_state = initial_state.clone();

    // Tables in reverse topological order (reduce dependent child tables first, then parent tables)
    let mut table_order = match schema.topological_order() {
        Ok(order) => order,
        Err(_) => schema.tables.iter().map(|t| t.name.clone()).collect(),
    };
    table_order.reverse();

    for table_name in table_order {
        if let Some(table_data) = current_state.tables.get(&table_name) {
            let row_count = table_data.rows.len();
            if row_count <= 1 {
                continue;
            }

            // Try 1-minimal row removal (remove one row at a time from the end or start)
            let mut idx = 0;
            while idx < current_state.tables[&table_name].rows.len() {
                // Do not reduce below 1 or 2 rows if that would make the table empty when it has data
                if current_state.tables[&table_name].rows.len() <= 1 {
                    break;
                }

                let mut candidate_state = current_state.clone();
                if let Some(t_data) = candidate_state.tables.get_mut(&table_name) {
                    t_data.rows.remove(idx);
                }

                // Clean up foreign key references if removing this row broke referential integrity
                clean_orphaned_fks(schema, &mut candidate_state);

                // Test if the reduced state still reproduces the failure
                if test_fn(&candidate_state).await {
                    current_state = candidate_state;
                    // Keep index same to test the new row that shifted into position
                } else {
                    idx += 1;
                }
            }
        }
    }

    current_state
}

pub async fn shrink_values<'a>(
    _schema: &DatabaseSchema,
    initial_state: &DatabaseState,
    test_fn: &TestFn<'a>,
) -> DatabaseState {
    let mut current_state = initial_state.clone();

    for (table_name, table_data) in initial_state.tables.iter() {
        for row_idx in 0..table_data.rows.len() {
            let cols: Vec<String> = table_data.rows[row_idx].values.keys().cloned().collect();

            for col_name in cols {
                let original_val = match current_state.tables[table_name].rows[row_idx]
                    .values
                    .get(&col_name)
                {
                    Some(v) => v.clone(),
                    None => continue,
                };

                if let SqlValue::Text(s) = &original_val {
                    if s.len() > 1 {
                        // Try shrinking string: e.g. "Berat@example.com" -> "B@example.com" or "a"
                        let mut candidates = Vec::new();

                        // If has '@', try single letter username
                        if let Some(at_pos) = s.find('@') {
                            if at_pos > 1 {
                                let first_char = &s[..1];
                                let domain = &s[at_pos..];
                                candidates.push(format!("{}{}", first_char, domain));
                            }
                        }

                        // Try trimmed
                        let trimmed = s.trim().to_string();
                        if &trimmed != s {
                            candidates.push(trimmed);
                        }

                        for cand in candidates {
                            let mut candidate_state = current_state.clone();
                            candidate_state.tables.get_mut(table_name).unwrap().rows[row_idx]
                                .values
                                .insert(col_name.clone(), SqlValue::Text(cand));

                            if test_fn(&candidate_state).await {
                                current_state = candidate_state;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    current_state
}

fn clean_orphaned_fks(schema: &DatabaseSchema, state: &mut DatabaseState) {
    // Collect all valid parent PK values
    let mut parent_pks: HashMap<String, HashSet<String>> = HashMap::new();

    for table in &schema.tables {
        if let Some(pk) = &table.primary_key {
            if let Some(pk_col) = pk.columns.first() {
                if let Some(t_data) = state.tables.get(&table.name) {
                    let mut pks = HashSet::new();
                    for row in &t_data.rows {
                        if let Some(val) = row.values.get(pk_col) {
                            pks.insert(val.to_sql_literal());
                        }
                    }
                    parent_pks.insert(table.name.clone(), pks);
                }
            }
        }
    }

    // For any table with FKs pointing to missing parents, remove the child rows
    for table in &schema.tables {
        for fk in &table.foreign_keys {
            if let (Some(fk_col), Some(_parent_col)) =
                (fk.columns.first(), fk.foreign_columns.first())
            {
                if let Some(valid_parents) = parent_pks.get(&fk.foreign_table) {
                    if let Some(t_data) = state.tables.get_mut(&table.name) {
                        t_data.rows.retain(|row| {
                            if let Some(val) = row.values.get(fk_col) {
                                valid_parents.contains(&val.to_sql_literal())
                            } else {
                                true
                            }
                        });
                    }
                }
            }
        }
    }
}
