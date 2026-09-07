use crate::db::client::PgClient;
use crate::error::Result;
use crate::schema::{
    CheckConstraint, Column, DataType, DatabaseSchema, EnumTypeDef, ForeignKey, Index, PrimaryKey,
    Table, UniqueConstraint,
};
use std::collections::HashMap;

struct FkIntrospectionRecord {
    columns: Vec<String>,
    foreign_table: String,
    foreign_columns: Vec<String>,
    on_delete: String,
    on_update: String,
}

pub struct SchemaInspector;

impl SchemaInspector {
    pub async fn introspect(
        client: &PgClient,
        target_table: Option<&str>,
    ) -> Result<DatabaseSchema> {
        let mut tables_map: HashMap<String, Table> = HashMap::new();

        // 1. Get base tables
        let query_tables = r#"
            SELECT table_name, table_schema
            FROM information_schema.tables
            WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
            ORDER BY table_name;
        "#;
        let rows = client.query(query_tables, &[]).await?;
        for row in rows {
            let table_name: String = row.get(0);
            let schema_name: String = row.get(1);

            if let Some(target) = target_table {
                if table_name != target {
                    continue;
                }
            }

            tables_map.insert(
                table_name.clone(),
                Table {
                    name: table_name,
                    schema_name,
                    columns: vec![],
                    primary_key: None,
                    foreign_keys: vec![],
                    unique_constraints: vec![],
                    check_constraints: vec![],
                    indexes: vec![],
                },
            );
        }

        // 2. Get columns
        let query_cols = r#"
            SELECT table_name, column_name, ordinal_position, column_default, is_nullable,
                   data_type, udt_name, character_maximum_length, numeric_precision, numeric_scale,
                   is_identity, is_generated
            FROM information_schema.columns
            WHERE table_schema = 'public'
            ORDER BY table_name, ordinal_position;
        "#;
        let rows = client.query(query_cols, &[]).await?;
        for row in rows {
            let table_name: String = row.get(0);
            if let Some(table) = tables_map.get_mut(&table_name) {
                let col_name: String = row.get(1);
                let default_val: Option<String> = row.get(3);
                let is_nullable_str: String = row.get(4);
                let udt_name: String = row.get(6);
                let char_max_len: Option<i32> = row.get(7);
                let num_prec: Option<i32> = row.get(8);
                let num_scale: Option<i32> = row.get(9);
                let is_identity_str: String = row.get(10);
                let is_gen_str: String = row.get(11);

                let data_type =
                    DataType::from_postgres_type(&udt_name, char_max_len, num_prec, num_scale);
                table.columns.push(Column {
                    name: col_name,
                    data_type,
                    is_nullable: is_nullable_str.eq_ignore_ascii_case("YES"),
                    default_value: default_val,
                    is_identity: is_identity_str.eq_ignore_ascii_case("YES"),
                    is_generated: !is_gen_str.eq_ignore_ascii_case("NEVER"),
                });
            }
        }

        // 3. Primary keys and Unique constraints
        let query_constraints = r#"
            SELECT
                tc.table_name,
                tc.constraint_name,
                tc.constraint_type,
                kcu.column_name,
                kcu.ordinal_position
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
              AND tc.table_schema = kcu.table_schema
            WHERE tc.table_schema = 'public'
              AND tc.constraint_type IN ('PRIMARY KEY', 'UNIQUE')
            ORDER BY tc.table_name, tc.constraint_name, kcu.ordinal_position;
        "#;
        let rows = client.query(query_constraints, &[]).await?;
        let mut grouped_constraints: HashMap<(String, String, String), Vec<String>> =
            HashMap::new();
        for row in rows {
            let table_name: String = row.get(0);
            let constr_name: String = row.get(1);
            let constr_type: String = row.get(2);
            let col_name: String = row.get(3);
            grouped_constraints
                .entry((table_name, constr_name, constr_type))
                .or_default()
                .push(col_name);
        }

        for ((table_name, constr_name, constr_type), columns) in grouped_constraints {
            if let Some(table) = tables_map.get_mut(&table_name) {
                if constr_type == "PRIMARY KEY" {
                    table.primary_key = Some(PrimaryKey {
                        name: constr_name,
                        columns,
                    });
                } else if constr_type == "UNIQUE" {
                    table.unique_constraints.push(UniqueConstraint {
                        name: constr_name,
                        columns,
                    });
                }
            }
        }

        // 4. Foreign keys
        let query_fks = r#"
            SELECT
                tc.table_name,
                tc.constraint_name,
                kcu.column_name,
                ccu.table_name AS foreign_table_name,
                ccu.column_name AS foreign_column_name,
                rc.delete_rule,
                rc.update_rule,
                kcu.ordinal_position
            FROM information_schema.table_constraints AS tc
            JOIN information_schema.key_column_usage AS kcu
              ON tc.constraint_name = kcu.constraint_name
              AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage AS ccu
              ON ccu.constraint_name = tc.constraint_name
              AND ccu.table_schema = tc.table_schema
            JOIN information_schema.referential_constraints AS rc
              ON rc.constraint_name = tc.constraint_name
              AND rc.constraint_schema = tc.table_schema
            WHERE tc.table_schema = 'public'
              AND tc.constraint_type = 'FOREIGN KEY'
            ORDER BY tc.table_name, tc.constraint_name, kcu.ordinal_position;
        "#;
        let rows = client.query(query_fks, &[]).await?;
        let mut fks_map: HashMap<(String, String), FkIntrospectionRecord> = HashMap::new();
        for row in rows {
            let table_name: String = row.get(0);
            let constr_name: String = row.get(1);
            let col_name: String = row.get(2);
            let foreign_table: String = row.get(3);
            let f_col: String = row.get(4);
            let on_delete: String = row.get(5);
            let on_update: String = row.get(6);

            let entry =
                fks_map
                    .entry((table_name, constr_name))
                    .or_insert_with(|| FkIntrospectionRecord {
                        columns: vec![],
                        foreign_table,
                        foreign_columns: vec![],
                        on_delete,
                        on_update,
                    });
            entry.columns.push(col_name);
            entry.foreign_columns.push(f_col);
        }

        for ((table_name, constr_name), record) in fks_map {
            if let Some(table) = tables_map.get_mut(&table_name) {
                table.foreign_keys.push(ForeignKey {
                    name: constr_name,
                    columns: record.columns,
                    foreign_table: record.foreign_table,
                    foreign_columns: record.foreign_columns,
                    on_delete: record.on_delete,
                    on_update: record.on_update,
                });
            }
        }

        // 5. Check constraints
        let query_checks = r#"
            SELECT
                tc.table_name,
                tc.constraint_name,
                cc.check_clause
            FROM information_schema.table_constraints tc
            JOIN information_schema.check_constraints cc
              ON tc.constraint_name = cc.constraint_name
              AND tc.constraint_schema = cc.constraint_schema
            WHERE tc.table_schema = 'public'
              AND tc.constraint_type = 'CHECK'
              AND cc.check_clause NOT LIKE '%IS NOT NULL%'
            ORDER BY tc.table_name, tc.constraint_name;
        "#;
        let rows = client.query(query_checks, &[]).await?;
        for row in rows {
            let table_name: String = row.get(0);
            let constr_name: String = row.get(1);
            let check_clause: String = row.get(2);

            if let Some(table) = tables_map.get_mut(&table_name) {
                table.check_constraints.push(CheckConstraint {
                    name: constr_name,
                    clause: check_clause,
                });
            }
        }

        // 6. Indexes
        let query_indexes = r#"
            SELECT
                t.relname AS table_name,
                i.relname AS index_name,
                ix.indisunique AS is_unique,
                pg_get_indexdef(ix.indexrelid) AS index_def
            FROM pg_index ix
            JOIN pg_class t ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE n.nspname = 'public'
              AND t.relkind = 'r'
            ORDER BY t.relname, i.relname;
        "#;
        let rows = client.query(query_indexes, &[]).await?;
        for row in rows {
            let table_name: String = row.get(0);
            let index_name: String = row.get(1);
            let is_unique: bool = row.get(2);
            let index_def: Option<String> = row.get(3);

            if let Some(table) = tables_map.get_mut(&table_name) {
                table.indexes.push(Index {
                    name: index_name,
                    columns: vec![],
                    is_unique,
                    definition: index_def,
                });
            }
        }

        // 7. Enums
        let query_enums = r#"
            SELECT
                t.typname AS enum_name,
                e.enumlabel AS enum_value
            FROM pg_type t
            JOIN pg_enum e ON t.oid = e.enumtypid
            JOIN pg_namespace n ON n.oid = t.typnamespace
            WHERE n.nspname = 'public'
            ORDER BY t.typname, e.enumsortorder;
        "#;
        let mut enums_map: HashMap<String, Vec<String>> = HashMap::new();
        if let Ok(rows) = client.query(query_enums, &[]).await {
            for row in rows {
                let enum_name: String = row.get(0);
                let enum_val: String = row.get(1);
                enums_map.entry(enum_name).or_default().push(enum_val);
            }
        }

        let mut enums = Vec::new();
        for (name, variants) in enums_map {
            enums.push(EnumTypeDef { name, variants });
        }

        let mut tables: Vec<Table> = tables_map.into_values().collect();
        tables.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(DatabaseSchema { tables, enums })
    }
}
