use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

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
pub(crate) struct CacheFile {
    pub projects: Vec<Project>,
    pub scanned_at: Option<String>,
}

pub struct CacheResult {
    pub projects: Vec<Project>,
    pub scanned_at: Option<String>,
}

const API_BASE: &str = "http://localhost:47891";

pub fn load_cache(path: &Path) -> Result<CacheResult> {
    let file = std::fs::File::open(path)?;
    let cache: CacheFile = serde_json::from_reader(file)?;
    Ok(CacheResult {
        projects: cache.projects,
        scanned_at: cache.scanned_at,
    })
}

pub fn refresh_projects() -> Result<Vec<Project>> {
    let url = format!("{API_BASE}/api/refresh");
    let mut resp = ureq::post(&url).send_empty()?;
    let cache: CacheFile = resp.body_mut().read_json()?;
    Ok(cache.projects)
}

/// Parse a subset of ISO 8601 timestamps and return human-readable age.
/// Handles "2026-02-02T02:42:38.008Z" format without chrono dependency.
pub fn format_cache_age(scanned_at: &str) -> String {
    let Some(secs) = parse_iso_epoch(scanned_at) else {
        return String::new();
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let delta = now.saturating_sub(secs);
    if delta < 60 {
        "just now".to_string()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86400)
    }
}

fn parse_iso_epoch(s: &str) -> Option<u64> {
    // "2026-02-02T02:42:38.008Z"
    let s = s.trim_end_matches('Z');
    let (date, time) = s.split_once('T')?;
    let mut date_parts = date.splitn(3, '-');
    let year: u64 = date_parts.next()?.parse().ok()?;
    let month: u64 = date_parts.next()?.parse().ok()?;
    let day: u64 = date_parts.next()?.parse().ok()?;

    let time_main = time.split('.').next()?;
    let mut time_parts = time_main.splitn(3, ':');
    let hour: u64 = time_parts.next()?.parse().ok()?;
    let min: u64 = time_parts.next()?.parse().ok()?;
    let sec: u64 = time_parts.next()?.parse().ok()?;

    // Days from epoch (1970) to the given date
    let days = days_from_epoch(year, month, day)?;
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

fn days_from_epoch(year: u64, month: u64, day: u64) -> Option<u64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Adjusted months: March=1 .. Feb=12
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe;
    // Epoch offset: 1970-01-01 = day 719468 in this civil calendar
    Some(days - 719468)
}
