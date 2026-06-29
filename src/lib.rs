//! tudo library crate: state, storage and rendering for the terminal todo app.
//!
//! Split out from the binary so the modules can be exercised by the integration
//! tests in `tests/` (and by anyone embedding the logic).

pub mod app;
pub mod clipboard;
pub mod config;
pub mod event;
pub mod model;
pub mod storage;
pub mod theme;
pub mod ui;
