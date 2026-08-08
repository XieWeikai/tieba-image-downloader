use crate::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, path::Path};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ItemStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemState {
    pub status: ItemStatus,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub retries: u32,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl Default for ItemState {
    fn default() -> Self {
        Self {
            status: ItemStatus::Pending,
            downloaded_bytes: 0,
            total_bytes: None,
            retries: 0,
            last_error: None,
            updated_at: Utc::now(),
        }
    }
}

pub type DownloadState = BTreeMap<String, ItemState>;

pub async fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(&tmp, data).await?;
    fs::rename(tmp, path).await?;
    Ok(())
}

pub async fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let data = fs::read(path).await?;
    serde_json::from_slice(&data).map_err(|source| AppError::StateCorrupt {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn atomically_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let value = BTreeMap::from([("a".to_owned(), ItemState::default())]);
        atomic_write_json(&path, &value).await.unwrap();
        let loaded: DownloadState = read_json(&path).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(!path.with_extension("json.tmp").exists());
    }
}
