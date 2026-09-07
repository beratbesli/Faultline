pub mod fingerprint;
pub mod introspection;

use crate::error::Result;
use petgraph::algo::toposort;
use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    SmallInt,
    Integer,
    BigInt,
    Numeric {
        precision: Option<u32>,
        scale: Option<u32>,
    },
    Real,
    DoublePrecision,
    Boolean,
    Text,
    Varchar(Option<u32>),
    Char(Option<u32>),
    Uuid,
    Date,
    Timestamp,
    TimestampTz,
    Json,
    Jsonb,
    Enum(String),
    Array(Box<DataType>),
    Other(String),
}

impl DataType {
    pub fn from_postgres_type(
        udt_name: &str,
        char_max_len: Option<i32>,
        num_prec: Option<i32>,
        num_scale: Option<i32>,
    ) -> Self {
        let udt = udt_name.to_lowercase();
        if let Some(elem_udt) = udt.strip_prefix('_') {
            let elem_type = Self::from_postgres_type(elem_udt, None, None, None);
            return DataType::Array(Box::new(elem_type));
        }

        match udt.as_str() {
            "int2" | "smallint" => DataType::SmallInt,
            "int4" | "integer" | "serial" => DataType::Integer,
            "int8" | "bigint" | "bigserial" => DataType::BigInt,
            "numeric" | "decimal" => DataType::Numeric {
                precision: num_prec.and_then(|p| if p > 0 { Some(p as u32) } else { None }),
                scale: num_scale.and_then(|s| if s >= 0 { Some(s as u32) } else { None }),
            },
            "float4" | "real" => DataType::Real,
            "float8" | "double precision" => DataType::DoublePrecision,
            "bool" | "boolean" => DataType::Boolean,
            "text" => DataType::Text,
            "varchar" => {
                DataType::Varchar(
                    char_max_len.and_then(|l| if l > 0 { Some(l as u32) } else { None }),
                )
            }
            "bpchar" | "char" => {
                DataType::Char(char_max_len.and_then(|l| if l > 0 { Some(l as u32) } else { None }))
            }
            "uuid" => DataType::Uuid,
            "date" => DataType::Date,
            "timestamp" => DataType::Timestamp,
            "timestamptz" => DataType::TimestampTz,
            "json" => DataType::Json,
            "jsonb" => DataType::Jsonb,
            _ => DataType::Other(udt_name.to_string()),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            DataType::SmallInt => "SMALLINT".to_string(),
            DataType::Integer => "INTEGER".to_string(),
            DataType::BigInt => "BIGINT".to_string(),
            DataType::Numeric { precision, scale } => match (precision, scale) {
                (Some(p), Some(s)) => format!("NUMERIC({}, {})", p, s),
                (Some(p), None) => format!("NUMERIC({})", p),
                _ => "NUMERIC".to_string(),
            },
            DataType::Real => "REAL".to_string(),
            DataType::DoublePrecision => "DOUBLE PRECISION".to_string(),
            DataType::Boolean => "BOOLEAN".to_string(),
            DataType::Text => "TEXT".to_string(),
            DataType::Varchar(Some(l)) => format!("VARCHAR({})", l),
            DataType::Varchar(None) => "VARCHAR".to_string(),
            DataType::Char(Some(l)) => format!("CHAR({})", l),
            DataType::Char(None) => "CHAR".to_string(),
            DataType::Uuid => "UUID".to_string(),
            DataType::Date => "DATE".to_string(),
            DataType::Timestamp => "TIMESTAMP".to_string(),
            DataType::TimestampTz => "TIMESTAMPTZ".to_string(),
            DataType::Json => "JSON".to_string(),
            DataType::Jsonb => "JSONB".to_string(),
            DataType::Enum(name) => format!("ENUM({})", name),
            DataType::Array(inner) => format!("{}[]", inner.display_name()),
            DataType::Other(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
    pub is_nullable: bool,
    pub default_value: Option<String>,
    pub is_identity: bool,
    pub is_generated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimaryKey {
    pub name: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub foreign_table: String,
    pub foreign_columns: Vec<String>,
    pub on_delete: String,
    pub on_update: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniqueConstraint {
    pub name: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckConstraint {
    pub name: String,
    pub clause: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub definition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumTypeDef {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub schema_name: String,
    pub columns: Vec<Column>,
    pub primary_key: Option<PrimaryKey>,
    pub foreign_keys: Vec<ForeignKey>,
    pub unique_constraints: Vec<UniqueConstraint>,
    pub check_constraints: Vec<CheckConstraint>,
    pub indexes: Vec<Index>,
}

impl Table {
    pub fn get_column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    pub fn is_pk_column(&self, name: &str) -> bool {
        self.primary_key
            .as_ref()
            .map(|pk| pk.columns.iter().any(|c| c == name))
            .unwrap_or(false)
    }

    pub fn is_unique_column(&self, name: &str) -> bool {
        if self.is_pk_column(name) {
            return true;
        }
        for uq in &self.unique_constraints {
            if uq.columns.len() == 1 && uq.columns[0] == name {
                return true;
            }
        }
        for idx in &self.indexes {
            if idx.is_unique && idx.columns.len() == 1 && idx.columns[0] == name {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatabaseSchema {
    pub tables: Vec<Table>,
    pub enums: Vec<EnumTypeDef>,
}

impl DatabaseSchema {
    pub fn get_table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    pub fn topological_order(&self) -> Result<Vec<String>> {
        let mut graph = DiGraph::<String, ()>::new();
        let mut node_map = HashMap::new();

        for table in &self.tables {
            let idx = graph.add_node(table.name.clone());
            node_map.insert(table.name.clone(), idx);
        }

        // Add directed edge: parent -> child (so parent is populated before child)
        for table in &self.tables {
            let child_idx = node_map[&table.name];
            for fk in &table.foreign_keys {
                if let Some(&parent_idx) = node_map.get(&fk.foreign_table) {
                    if parent_idx != child_idx {
                        graph.add_edge(parent_idx, child_idx, ());
                    }
                }
            }
        }

        match toposort(&graph, None) {
            Ok(order) => Ok(order.into_iter().map(|idx| graph[idx].clone()).collect()),
            Err(_) => {
                // If there's a cyclic foreign key, fallback to original order
                Ok(self.tables.iter().map(|t| t.name.clone()).collect())
            }
        }
    }

    pub fn fingerprint(&self) -> String {
        fingerprint::compute_schema_fingerprint(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_order_simple() {
        let parent = Table {
            name: "users".to_string(),
            schema_name: "public".to_string(),
            columns: vec![Column {
                name: "id".to_string(),
                data_type: DataType::Integer,
                is_nullable: false,
                default_value: None,
                is_identity: true,
                is_generated: false,
            }],
            primary_key: Some(PrimaryKey {
                name: "pk_users".to_string(),
                columns: vec!["id".to_string()],
            }),
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
        };

        let child = Table {
            name: "orders".to_string(),
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
                    name: "user_id".to_string(),
                    data_type: DataType::Integer,
                    is_nullable: false,
                    default_value: None,
                    is_identity: false,
                    is_generated: false,
                },
            ],
            primary_key: Some(PrimaryKey {
                name: "pk_orders".to_string(),
                columns: vec!["id".to_string()],
            }),
            foreign_keys: vec![ForeignKey {
                name: "fk_orders_user".to_string(),
                columns: vec!["user_id".to_string()],
                foreign_table: "users".to_string(),
                foreign_columns: vec!["id".to_string()],
                on_delete: "NO ACTION".to_string(),
                on_update: "NO ACTION".to_string(),
            }],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
        };

        // Notice child is placed first in the list, but toposort should order parent before child
        let schema = DatabaseSchema {
            tables: vec![child, parent],
            enums: vec![],
        };

        let order = schema.topological_order().expect("Toposort failed");
        assert_eq!(order, vec!["users".to_string(), "orders".to_string()]);
    }

    #[test]
    fn test_schema_fingerprint_deterministic() {
        let table = Table {
            name: "users".to_string(),
            schema_name: "public".to_string(),
            columns: vec![Column {
                name: "email".to_string(),
                data_type: DataType::Text,
                is_nullable: false,
                default_value: None,
                is_identity: false,
                is_generated: false,
            }],
            primary_key: None,
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
        };

        let schema1 = DatabaseSchema {
            tables: vec![table.clone()],
            enums: vec![],
        };
        let schema2 = DatabaseSchema {
            tables: vec![table],
            enums: vec![],
        };

        assert_eq!(schema1.fingerprint(), schema2.fingerprint());
    }
}

impl DatabaseSchema {
    pub fn generate_create_ddl(&self) -> String {
        let mut ddl = String::new();

        // 1. Enums
        for e in &self.enums {
            let variants: Vec<String> = e.variants.iter().map(|v| format!("'{}'", v)).collect();
            ddl.push_str(&format!(
                "CREATE TYPE \"{}\" AS ENUM ({});\n",
                e.name,
                variants.join(", ")
            ));
        }

        // 2. Tables in topological order
        let table_order = self
            .topological_order()
            .unwrap_or_else(|_| self.tables.iter().map(|t| t.name.clone()).collect());

        for table_name in table_order {
            if let Some(table) = self.get_table(&table_name) {
                ddl.push_str(&format!("CREATE TABLE \"{}\" (\n", table.name));
                let mut defs = Vec::new();

                for col in &table.columns {
                    let mut col_def =
                        format!("  \"{}\" {}", col.name, col.data_type.display_name());
                    if !col.is_nullable {
                        col_def.push_str(" NOT NULL");
                    }
                    if let Some(default_val) = &col.default_value {
                        col_def.push_str(&format!(" DEFAULT {}", default_val));
                    }
                    defs.push(col_def);
                }

                if let Some(pk) = &table.primary_key {
                    let cols: Vec<String> =
                        pk.columns.iter().map(|c| format!("\"{}\"", c)).collect();
                    defs.push(format!(
                        "  CONSTRAINT \"{}\" PRIMARY KEY ({})",
                        pk.name,
                        cols.join(", ")
                    ));
                }

                for uq in &table.unique_constraints {
                    let cols: Vec<String> =
                        uq.columns.iter().map(|c| format!("\"{}\"", c)).collect();
                    defs.push(format!(
                        "  CONSTRAINT \"{}\" UNIQUE ({})",
                        uq.name,
                        cols.join(", ")
                    ));
                }

                for fk in &table.foreign_keys {
                    let cols: Vec<String> =
                        fk.columns.iter().map(|c| format!("\"{}\"", c)).collect();
                    let fcols: Vec<String> = fk
                        .foreign_columns
                        .iter()
                        .map(|c| format!("\"{}\"", c))
                        .collect();
                    defs.push(format!(
                        "  CONSTRAINT \"{}\" FOREIGN KEY ({}) REFERENCES \"{}\" ({}) ON DELETE {} ON UPDATE {}",
                        fk.name, cols.join(", "), fk.foreign_table, fcols.join(", "), fk.on_delete, fk.on_update
                    ));
                }

                for ck in &table.check_constraints {
                    defs.push(format!(
                        "  CONSTRAINT \"{}\" CHECK ({})",
                        ck.name, ck.clause
                    ));
                }

                ddl.push_str(&defs.join(",\n"));
                ddl.push_str("\n);\n\n");
            }
        }

        ddl
    }
}
