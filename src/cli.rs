use clap::Parser;
use dialoguer::{Confirm, Input, Select};
use std::path::PathBuf;

use crate::{AppError, Result, config::Config, report::OutputFormat, tieba::extract_thread_id};

fn parse_image_concurrency(value: &str) -> std::result::Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|v| (1..=256).contains(v))
        .ok_or_else(|| "并发数必须为 1-256".to_owned())
}

fn parse_page_concurrency(value: &str) -> std::result::Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|v| (1..=64).contains(v))
        .ok_or_else(|| "页面并发数必须为 1-64".to_owned())
}

#[derive(Debug, Parser)]
#[command(version, about = "百度贴吧原图批量下载工具")]
pub struct Args {
    /// 帖子链接；省略时进入交互向导
    pub url: Option<String>,
    #[arg(long)]
    pub only_author: bool,
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(long, default_value_t = 32, value_parser = parse_image_concurrency)]
    pub concurrency: usize,
    #[arg(long, default_value_t = 8, value_parser = parse_page_concurrency)]
    pub page_concurrency: usize,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub auto_concurrency: bool,
    #[arg(long)]
    pub cookie_file: Option<PathBuf>,
    #[arg(long, default_value_t = 4)]
    pub retries: u32,
    /// 自动调节时每个预热批次之间的延迟
    #[arg(long, default_value_t = 750)]
    pub warmup_delay_ms: u64,
    /// 403/429 后的最短冷却时间
    #[arg(long, default_value_t = 30)]
    pub cooldown_secs: u64,
    /// 安全验证时自动打开专用 Chrome 登录窗口
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub browser_login: bool,
    /// 自定义 Chrome/Chromium 可执行文件路径
    #[arg(long)]
    pub chrome_path: Option<PathBuf>,
    /// 等待用户完成浏览器验证的最长秒数
    #[arg(long, default_value_t = 600)]
    pub login_timeout_secs: u64,
    /// 将验证后的百度会话保存到 macOS 钥匙串
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub remember_login: bool,
    /// 清除钥匙串中的百度会话后再运行
    #[arg(long)]
    pub clear_login: bool,
    /// 将认证后的页面 HTML 保存到指定目录（仅用于解析调试）
    #[arg(long, hide = true)]
    pub diagnostic_html_dir: Option<PathBuf>,
    /// 最终结果格式；json 模式只在 stdout 输出一份 JSON
    #[arg(long, value_enum, default_value_t)]
    pub output_format: OutputFormat,
    /// 只解析并写入 manifest.json，不下载图片
    #[arg(long)]
    pub metadata_only: bool,
}

fn default_download_dir(thread_id: u64) -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Downloads")
        .join(format!("tieba_{thread_id}"))
}

pub fn collect() -> Result<Config> {
    let args = Args::parse();
    if let Some(url) = args.url {
        let id = extract_thread_id(&url)?;
        return Ok(Config {
            thread_url: url,
            only_author: args.only_author,
            output_dir: args.output.unwrap_or_else(|| default_download_dir(id)),
            image_concurrency: args.concurrency,
            page_concurrency: args.page_concurrency,
            auto_concurrency: args.auto_concurrency,
            cookie_file: args.cookie_file,
            retries: args.retries,
            warmup_delay_ms: args.warmup_delay_ms,
            cooldown_secs: args.cooldown_secs,
            browser_login: args.browser_login,
            chrome_path: args.chrome_path,
            login_timeout_secs: args.login_timeout_secs,
            remember_login: args.remember_login,
            clear_login: args.clear_login,
            diagnostic_html_dir: args.diagnostic_html_dir,
            output_format: args.output_format,
            metadata_only: args.metadata_only,
        });
    }
    let url: String = Input::new()
        .with_prompt("贴吧帖子链接")
        .interact_text()
        .map_err(|e| AppError::PageAccess(e.to_string()))?;
    let id = extract_thread_id(&url)?;
    let mode = Select::new()
        .with_prompt("下载范围")
        .items(&["全帖所有楼层", "只看楼主"])
        .default(0)
        .interact()
        .map_err(|e| AppError::PageAccess(e.to_string()))?;
    let default = default_download_dir(id).display().to_string();
    let output: String = Input::new()
        .with_prompt("保存目录")
        .default(default)
        .interact_text()
        .map_err(|e| AppError::PageAccess(e.to_string()))?;
    let image_concurrency: usize = Input::new()
        .with_prompt("图片下载并发数 (1-256)")
        .default(32)
        .validate_with(|v: &usize| {
            if (1..=256).contains(v) {
                Ok(())
            } else {
                Err("必须为 1-256")
            }
        })
        .interact_text()
        .map_err(|e| AppError::PageAccess(e.to_string()))?;
    let page_concurrency: usize = Input::new()
        .with_prompt("页面扫描并发数 (1-64)")
        .default(8)
        .validate_with(|v: &usize| {
            if (1..=64).contains(v) {
                Ok(())
            } else {
                Err("必须为 1-64")
            }
        })
        .interact_text()
        .map_err(|e| AppError::PageAccess(e.to_string()))?;
    let auto_concurrency = Confirm::new()
        .with_prompt("启用自动并发调节")
        .default(true)
        .interact()
        .map_err(|e| AppError::PageAccess(e.to_string()))?;
    let browser_login = Confirm::new()
        .with_prompt("安全验证时自动打开专用 Chrome 登录窗口")
        .default(true)
        .interact()
        .map_err(|e| AppError::PageAccess(e.to_string()))?;
    let cookie: String = Input::new()
        .with_prompt("Cookie 文件路径（可留空）")
        .allow_empty(true)
        .interact_text()
        .map_err(|e| AppError::PageAccess(e.to_string()))?;
    Ok(Config {
        thread_url: url,
        only_author: mode == 1,
        output_dir: PathBuf::from(output),
        image_concurrency,
        page_concurrency,
        auto_concurrency,
        cookie_file: (!cookie.trim().is_empty()).then(|| PathBuf::from(cookie)),
        retries: 4,
        warmup_delay_ms: 750,
        cooldown_secs: 30,
        browser_login,
        chrome_path: None,
        login_timeout_secs: 600,
        remember_login: true,
        clear_login: false,
        diagnostic_html_dir: None,
        output_format: args.output_format,
        metadata_only: args.metadata_only,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_machine_output_and_metadata_mode() {
        let args = Args::try_parse_from([
            "tieba-image-downloader",
            "https://tieba.baidu.com/p/10918721568",
            "--output-format",
            "json",
            "--metadata-only",
        ])
        .unwrap();
        assert_eq!(args.output_format, OutputFormat::Json);
        assert!(args.metadata_only);
    }
}
