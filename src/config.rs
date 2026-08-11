use std::path::PathBuf;

use crate::report::OutputFormat;

#[derive(Debug, Clone)]
pub struct Config {
    pub thread_url: String,
    pub only_author: bool,
    pub output_dir: PathBuf,
    pub image_concurrency: usize,
    pub page_concurrency: usize,
    pub auto_concurrency: bool,
    pub cookie_file: Option<PathBuf>,
    pub retries: u32,
    pub warmup_delay_ms: u64,
    pub cooldown_secs: u64,
    pub browser_login: bool,
    pub chrome_path: Option<PathBuf>,
    pub login_timeout_secs: u64,
    pub remember_login: bool,
    pub clear_login: bool,
    pub diagnostic_html_dir: Option<PathBuf>,
    pub output_format: OutputFormat,
    pub metadata_only: bool,
}
