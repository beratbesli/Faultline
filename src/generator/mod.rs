pub mod primitive;
pub mod relation;
pub mod seed;

use crate::error::Result;
use crate::schema::DatabaseSchema;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlValue {
    Null,
    Bool(bool),
    SmallInt(i16),
    Integer(i32),
    BigInt(i64),
    Float(f64),
    Numeric(String),
    Text(String),
    Uuid(String),
    Date(String),
    Timestamp(String),
    Json(String),
    Array(Vec<SqlValue>),
}

impl SqlValue {
    pub fn to_sql_literal(&self) -> String {
        match self {
            SqlValue::Null => "NULL".to_string(),
            SqlValue::Bool(b) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            SqlValue::SmallInt(i) => i.to_string(),
            SqlValue::Integer(i) => i.to_string(),
            SqlValue::BigInt(i) => i.to_string(),
            SqlValue::Float(f) => {
                if f.is_nan() {
                    "'NaN'".to_string()
                } else if f.is_infinite() {
                    if *f > 0.0 {
                        "'Infinity'".to_string()
                    } else {
                        "'-Infinity'".to_string()
                    }
                } else {
                    f.to_string()
                }
            }
            SqlValue::Numeric(s) => s.clone(),
            SqlValue::Text(s) => {
                let escaped = s.replace('\'', "''");
                format!("'{}'", escaped)
            }
            SqlValue::Uuid(s) => format!("'{}'", s),
            SqlValue::Date(s) => format!("'{}'", s),
            SqlValue::Timestamp(s) => format!("'{}'", s),
            SqlValue::Json(s) => {
                let escaped = s.replace('\'', "''");
                format!("'{}'", escaped)
            }
            SqlValue::Array(items) => {
                let inner: Vec<String> = items.iter().map(|item| item.to_sql_literal()).collect();
                format!("ARRAY[{}]", inner.join(", "))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RowData {
    pub values: HashMap<String, SqlValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TableData {
    pub table_name: String,
    pub rows: Vec<RowData>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DatabaseState {
    pub tables: HashMap<String, TableData>,
}

impl DatabaseState {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    pub fn total_rows(&self) -> usize {
        self.tables.values().map(|t| t.rows.len()).sum()
    }

    pub fn to_insert_sql(&self, schema: &DatabaseSchema) -> Result<String> {
        let mut sql = String::new();
        let order = schema.topological_order()?;

        for table_name in order {
            if let Some(table_data) = self.tables.get(&table_name) {
                if table_data.rows.is_empty() {
                    continue;
                }

                let table = match schema.get_table(&table_name) {
                    Some(t) => t,
                    None => continue,
                };

                // Get insertable column names
                let cols: Vec<String> = table
                    .columns
                    .iter()
                    .filter(|c| !c.is_generated)
                    .map(|c| c.name.clone())
                    .collect();

                if cols.is_empty() {
                    continue;
                }

                for row in &table_data.rows {
                    let mut col_names = Vec::new();
                    let mut val_literals = Vec::new();

                    for col_name in &cols {
                        if let Some(val) = row.values.get(col_name) {
                            col_names.push(format!("\"{}\"", col_name));
                            val_literals.push(val.to_sql_literal());
                        }
                    }

                    if !col_names.is_empty() {
                        sql.push_str(&format!(
                            "INSERT INTO \"{}\" ({}) VALUES ({});\n",
                            table_name,
                            col_names.join(", "),
                            val_literals.join(", ")
                        ));
                    }
                }
            }
        }

        Ok(sql)
    }

    pub fn fingerprint(&self) -> String {
        let mut sorted_tables: Vec<&String> = self.tables.keys().collect();
        sorted_tables.sort();

        let mut hasher = sha2::Sha256::default();
        use sha2::Digest;

        for table_name in sorted_tables {
            hasher.update(table_name.as_bytes());
            if let Some(table_data) = self.tables.get(table_name) {
                for row in &table_data.rows {
                    let mut sorted_cols: Vec<&String> = row.values.keys().collect();
                    sorted_cols.sort();
                    for col in sorted_cols {
                        hasher.update(col.as_bytes());
                        let val_str = row.values[col].to_sql_literal();
                        hasher.update(val_str.as_bytes());
                    }
                }
            }
        }

        hex::encode(hasher.finalize())
    }
}

pub trait ValueGenerator {
    fn generate_primitive(
        &mut self,
        data_type: &crate::schema::DataType,
        rng: &mut ChaCha8Rng,
    ) -> SqlValue;
}
