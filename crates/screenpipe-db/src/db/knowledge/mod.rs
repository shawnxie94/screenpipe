// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Local Knowledge persistence: source registry, work units, knowledge versions,
//! jobs, deletions, history migration and search projection. All writes go
//! through `DatabaseManager` write coordination — no second pool, ever.

use super::*;

pub mod deletion;
pub mod distill;
pub mod history;
pub mod jobs;
pub mod items;
pub mod office;
pub mod search;
pub mod sources;
pub mod state;
pub mod trace;
pub mod types;
pub mod work_units;

#[cfg(test)]
mod tests;
