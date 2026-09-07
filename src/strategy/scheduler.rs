use crate::error::{FaultlineError, Result};
use crate::migration::analyzer::MigrationHints;
use crate::strategy::boundary::BoundaryStrategy;
use crate::strategy::collision::CollisionStrategy;
use crate::strategy::nullability::NullabilityStrategy;
use crate::strategy::precision::PrecisionStrategy;
use crate::strategy::random::RandomValidStrategy;
use crate::strategy::SearchStrategy;
use std::sync::Arc;

pub struct StrategyScheduler {
    strategies: Vec<Arc<dyn SearchStrategy>>,
}

impl StrategyScheduler {
    pub fn new(configured_names: &[String], hints: Option<&MigrationHints>) -> Result<Self> {
        let mut strategy_names = Vec::new();

        if !configured_names.is_empty() {
            strategy_names.extend(configured_names.iter().cloned());
        } else if let Some(h) = hints {
            strategy_names.extend(h.suggested_strategies.iter().cloned());
        }

        if strategy_names.is_empty() {
            strategy_names = vec![
                "collision".to_string(),
                "precision".to_string(),
                "nullability".to_string(),
                "boundary".to_string(),
                "random".to_string(),
            ];
        }

        let mut strategies: Vec<Arc<dyn SearchStrategy>> = Vec::new();
        for name in &strategy_names {
            match name.to_lowercase().as_str() {
                "collision" => strategies.push(Arc::new(CollisionStrategy)),
                "precision" => strategies.push(Arc::new(PrecisionStrategy)),
                "nullability" => strategies.push(Arc::new(NullabilityStrategy)),
                "boundary" => strategies.push(Arc::new(BoundaryStrategy)),
                "random" => strategies.push(Arc::new(RandomValidStrategy)),
                unknown => {
                    return Err(FaultlineError::Config(format!(
                        "Unknown search strategy '{}'. Valid strategies: collision, precision, nullability, boundary, random",
                        unknown
                    )));
                }
            }
        }

        Ok(Self { strategies })
    }

    pub fn strategies(&self) -> &[Arc<dyn SearchStrategy>] {
        &self.strategies
    }

    pub fn get_strategy(&self, index: usize) -> Arc<dyn SearchStrategy> {
        self.strategies[index % self.strategies.len()].clone()
    }
}
