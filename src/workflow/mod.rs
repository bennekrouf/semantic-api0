pub mod actions;
mod config;
pub mod context;
pub mod engine;

pub use actions::*;
pub use config::WorkflowConfig;
pub use context::WorkflowContext;
pub use engine::WorkflowEngine;

use async_trait::async_trait;
use std::error::Error;

#[async_trait]
pub trait WorkflowStep: Send + Sync {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn name(&self) -> &'static str;
}
pub mod steps;
