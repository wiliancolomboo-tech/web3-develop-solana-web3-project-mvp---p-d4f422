// programs/univibe_protocol/src/instructions/mod.rs
pub mod initialize_grant;
pub mod submit_milestone;
pub mod approve_milestone;
pub mod release_funds;

pub use initialize_grant::*;
pub use submit_milestone::*;
pub use approve_milestone::*;
pub use release_funds::*;