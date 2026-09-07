use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationSeed {
    pub value: u64,
}

impl GenerationSeed {
    pub fn new(value: u64) -> Self {
        Self { value }
    }

    pub fn to_rng(&self) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(self.value)
    }

    pub fn derive_subseed(&self, index: u64) -> ChaCha8Rng {
        let combined = self
            .value
            .wrapping_mul(6364136223846793005)
            .wrapping_add(index);
        ChaCha8Rng::seed_from_u64(combined)
    }
}
