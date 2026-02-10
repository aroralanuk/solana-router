//! Convex optimization router for Prop AMMs
//!
//! Uses dual decomposition with L-BFGS-B to find optimal trade splits
//! across black-box Prop AMM pools.

pub mod baseline;
pub mod bisection;
pub mod quoter;
pub mod router;
pub mod types;

pub use baseline::BellmanFordRouter;
pub use bisection::{BisectionConfig, BisectionResult, find_optimal_amount};
pub use quoter::Quoter;
pub use router::{ConvexRouter, RouterConfig};
pub use types::*;
