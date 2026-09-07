use crate::generator::SqlValue;
use crate::schema::DataType;
use rand::Rng;
use rand_chacha::ChaCha8Rng;

pub struct PrimitiveGenerator;

impl PrimitiveGenerator {
    pub fn generate_value(
        data_type: &DataType,
        is_nullable: bool,
        allow_null: bool,
        rng: &mut ChaCha8Rng,
    ) -> SqlValue {
        if is_nullable && allow_null && rng.gen_bool(0.15) {
            return SqlValue::Null;
        }

        match data_type {
            DataType::SmallInt => {
                let choice: u32 = rng.gen_range(0..5);
                match choice {
                    0 => SqlValue::SmallInt(0),
                    1 => SqlValue::SmallInt(1),
                    2 => SqlValue::SmallInt(-1),
                    3 => SqlValue::SmallInt(i16::MAX),
                    _ => SqlValue::SmallInt(rng.gen_range(1..1000)),
                }
            }
            DataType::Integer => {
                let choice: u32 = rng.gen_range(0..6);
                match choice {
                    0 => SqlValue::Integer(0),
                    1 => SqlValue::Integer(1),
                    2 => SqlValue::Integer(-1),
                    3 => SqlValue::Integer(i32::MAX),
                    4 => SqlValue::Integer(i32::MIN),
                    _ => SqlValue::Integer(rng.gen_range(1..100_000)),
                }
            }
            DataType::BigInt => {
                let choice: u32 = rng.gen_range(0..5);
                match choice {
                    0 => SqlValue::BigInt(0),
                    1 => SqlValue::BigInt(1),
                    2 => SqlValue::BigInt(-1),
                    3 => SqlValue::BigInt(i64::MAX),
                    _ => SqlValue::BigInt(rng.gen_range(1..1_000_000)),
                }
            }
            DataType::Numeric { precision, scale } => {
                let s = scale.unwrap_or(2);
                let _p = precision.unwrap_or(10);
                // Deliberately include fractional numbers like 19.99 for precision tests
                let choice: u32 = rng.gen_range(0..4);
                match choice {
                    0 => SqlValue::Numeric("19.99".to_string()),
                    1 => SqlValue::Numeric("0.50".to_string()),
                    2 => SqlValue::Numeric("100.00".to_string()),
                    _ => {
                        let int_part = rng.gen_range(1..1000);
                        let frac_part = if s > 0 { rng.gen_range(1..99) } else { 0 };
                        SqlValue::Numeric(format!("{}.{:02}", int_part, frac_part))
                    }
                }
            }
            DataType::Real | DataType::DoublePrecision => {
                let choice: u32 = rng.gen_range(0..4);
                match choice {
                    0 => SqlValue::Float(0.0),
                    1 => SqlValue::Float(19.99),
                    2 => SqlValue::Float(-1.0),
                    _ => SqlValue::Float(rng.gen_range(1.0..1000.0)),
                }
            }
            DataType::Boolean => SqlValue::Bool(rng.gen_bool(0.5)),
            DataType::Text | DataType::Varchar(_) | DataType::Char(_) => {
                let max_len = match data_type {
                    DataType::Varchar(Some(l)) => *l as usize,
                    DataType::Char(Some(l)) => *l as usize,
                    _ => 255,
                };
                Self::generate_text_value(max_len, rng)
            }
            DataType::Uuid => {
                let u = uuid::Uuid::from_u128(rng.gen());
                SqlValue::Uuid(u.to_string())
            }
            DataType::Date => {
                let days = rng.gen_range(1..28);
                let months = rng.gen_range(1..12);
                let year = rng.gen_range(2020..2030);
                SqlValue::Date(format!("{:04}-{:02}-{:02}", year, months, days))
            }
            DataType::Timestamp | DataType::TimestampTz => {
                let days = rng.gen_range(1..28);
                let months = rng.gen_range(1..12);
                let year = rng.gen_range(2020..2030);
                let h = rng.gen_range(0..23);
                let m = rng.gen_range(0..59);
                let s = rng.gen_range(0..59);
                SqlValue::Timestamp(format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    year, months, days, h, m, s
                ))
            }
            DataType::Json | DataType::Jsonb => {
                let id = rng.gen_range(1..100);
                SqlValue::Json(format!("{{\"id\": {}, \"active\": true}}", id))
            }
            DataType::Enum(name) => {
                // If enum name known, fallback to generic variant
                SqlValue::Text(format!("{}_v1", name))
            }
            DataType::Array(inner) => {
                let elem = Self::generate_value(inner, false, false, rng);
                SqlValue::Array(vec![elem])
            }
            DataType::Other(_) => SqlValue::Text("sample_value".to_string()),
        }
    }

    pub fn generate_text_value(max_len: usize, rng: &mut ChaCha8Rng) -> SqlValue {
        let sample_emails = [
            "Berat@example.com",
            "berat@example.com",
            "Alice@example.com",
            "alice@example.com",
            "bob@domain.org",
            " admin@system.net ",
            "user+test@service.co",
            "Café@international.com",
        ];

        let choice = rng.gen_range(0..sample_emails.len() + 3);
        let val = if choice < sample_emails.len() {
            sample_emails[choice].to_string()
        } else if choice == sample_emails.len() {
            "".to_string()
        } else if choice == sample_emails.len() + 1 {
            "a".to_string()
        } else {
            format!("user_{}@example.com", rng.gen_range(1000..9999))
        };

        if val.len() > max_len && max_len > 0 {
            SqlValue::Text(val[..max_len].to_string())
        } else {
            SqlValue::Text(val)
        }
    }
}
