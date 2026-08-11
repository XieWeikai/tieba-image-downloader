use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunSummary {
    pub post_id: String,
    pub output_dir: String,
    pub discovered: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub browser_verification_used: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_stable_machine_contract() {
        let summary = RunSummary {
            post_id: "10918721568".into(),
            output_dir: "/tmp/tieba_10918721568".into(),
            discovered: 216,
            completed: 214,
            skipped: 2,
            failed: 0,
            browser_verification_used: true,
        };
        let value = serde_json::to_value(summary).unwrap();
        assert_eq!(value["post_id"], "10918721568");
        assert_eq!(value["discovered"], 216);
        assert_eq!(value["browser_verification_used"], true);
        assert_eq!(value.as_object().unwrap().len(), 7);
    }
}
