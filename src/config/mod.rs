//! Researching configuration
//! and setting it.

use std::{path::PathBuf, sync::OnceLock};

static CONFIG: OnceLock<String> = OnceLock::new();

pub struct RefreshingConfigFile {
    path: PathBuf,
}