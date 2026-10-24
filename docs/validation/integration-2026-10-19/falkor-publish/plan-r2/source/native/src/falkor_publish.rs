//! Explicit FalkorDB publication plans. Preparing data opens no connection.
#[path = "falkor_publish_plan.rs"]
mod plan;
#[path = "falkor_publish_program.rs"]
mod program;
pub use crate::neo4j_publish::Limits as PayloadLimits;
pub use plan::{Mode, Plan, Target};
pub use program::{Operation, command, operations};
