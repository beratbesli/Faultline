use crate::error::Result;
use crate::generator::relation::StateGenerator;
use crate::generator::DatabaseState;
use crate::schema::DatabaseSchema;
use crate::strategy::SearchStrategy;
use rand_chacha::ChaCha8Rng;

pub struct RandomValidStrategy;

impl SearchStrategy for RandomValidStrategy {
    fn name(&self) -> &'static str {
        "random"
    }

    fn generate_candidate(
        &self,
        schema: &DatabaseSchema,
        target_rows: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<DatabaseState> {
        StateGenerator::generate_valid_state(schema, target_rows, rng)
    }
}
