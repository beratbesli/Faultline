pub mod boundary;
pub mod collision;
pub mod nullability;
pub mod precision;
pub mod random;
pub mod scheduler;

use crate::error::Result;
use crate::generator::DatabaseState;
use crate::schema::DatabaseSchema;
use rand_chacha::ChaCha8Rng;

pub trait SearchStrategy: Send + Sync {
    fn name(&self) -> &'static str;
    fn generate_candidate(
        &self,
        schema: &DatabaseSchema,
        target_rows: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<DatabaseState>;
}

pub use scheduler::StrategyScheduler;
