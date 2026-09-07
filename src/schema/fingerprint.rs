use crate::schema::DatabaseSchema;
use sha2::{Digest, Sha256};

pub fn compute_schema_fingerprint(schema: &DatabaseSchema) -> String {
    let mut hasher = Sha256::new();

    // Sort tables by name for deterministic ordering
    let mut sorted_tables = schema.tables.clone();
    sorted_tables.sort_by(|a, b| a.name.cmp(&b.name));

    for table in &sorted_tables {
        hasher.update(b"table:");
        hasher.update(table.name.as_bytes());
        hasher.update(b"\n");

        for col in &table.columns {
            hasher.update(b"  col:");
            hasher.update(col.name.as_bytes());
            hasher.update(b":");
            hasher.update(col.data_type.display_name().as_bytes());
            hasher.update(b":null=");
            hasher.update(if col.is_nullable { b"1" } else { b"0" });
            hasher.update(b"\n");
        }

        if let Some(pk) = &table.primary_key {
            hasher.update(b"  pk:");
            hasher.update(pk.columns.join(",").as_bytes());
            hasher.update(b"\n");
        }

        for uq in &table.unique_constraints {
            hasher.update(b"  uq:");
            hasher.update(uq.columns.join(",").as_bytes());
            hasher.update(b"\n");
        }

        for fk in &table.foreign_keys {
            hasher.update(b"  fk:");
            hasher.update(fk.columns.join(",").as_bytes());
            hasher.update(b"->");
            hasher.update(fk.foreign_table.as_bytes());
            hasher.update(b"(");
            hasher.update(fk.foreign_columns.join(",").as_bytes());
            hasher.update(b")\n");
        }

        for ck in &table.check_constraints {
            hasher.update(b"  ck:");
            hasher.update(ck.clause.as_bytes());
            hasher.update(b"\n");
        }
    }

    let result = hasher.finalize();
    hex::encode(result)
}
