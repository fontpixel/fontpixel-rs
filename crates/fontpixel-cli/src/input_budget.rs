//! Cumulative retained-model limits across CLI inputs now live in
//! `fontpixel-ops`. This path is kept for the `tests/input_budget.rs` regression
//! suite, which includes it by path; the binary itself does not use it.
pub(crate) use fontpixel_ops::input_budget::InputBudget;
