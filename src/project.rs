use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub path: String,
    pub relative_path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub project_type: Option<String>,
    pub framework: Option<String>,
    pub modified_at: Option<String>,
    pub runner: Option<String>,
    pub dev_command: Option<String>,
    pub git: Option<String>,
    pub git_branch: Option<String>,
    #[serde(default)]
    pub scripts: Option<HashMap<String, String>>,
    #[serde(default)]
    pub has_justfile: Option<bool>,
    #[serde(default)]
    pub just_recipes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheFile {
    projects: Vec<Project>,
}

pub fn load_cache(path: &Path) -> Result<Vec<Project>> {
    let file = std::fs::File::open(path)?;
    let cache: CacheFile = serde_json::from_reader(file)?;
    Ok(cache.projects)
}
