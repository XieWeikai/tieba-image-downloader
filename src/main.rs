use futures_util::{StreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{
    Client, StatusCode,
    header::{COOKIE, HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT},
};
use std::{collections::VecDeque, sync::Arc, time::Duration};
use tieba_image_downloader::{
    AppError, Result, adaptive, chrome_auth, cli,
    config::Config,
    cookie::load_cookie,
    downloader::{DownloadOutcome, download_one},
    keychain,
    parser::{
        ImageRecord, api_total_pages, looks_like_client_rendered_shell, looks_like_verification,
        parse_api_page, parse_page, sort_deduplicate, total_pages,
    },
    state::{DownloadState, ItemState, ItemStatus, atomic_write_json},
    tieba::{extract_thread_id, page_url},
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

fn build_client(cookie: Option<String>) -> Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/124 Safari/537.36"));
    if let Some(value) = cookie {
        headers.insert(
            COOKIE,
            HeaderValue::from_str(value.trim())
                .map_err(|_| AppError::InvalidCookie("包含无效请求头字符".into()))?,
        );
    }
    Ok(Client::builder()
        .default_headers(headers)
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .pool_idle_timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?)
}

async fn fetch_page(
    client: &Client,
    id: u64,
    page: usize,
    only_author: bool,
    retries: u32,
    cooldown_secs: u64,
) -> Result<String> {
    let url = page_url(id, page, only_author);
    let mut last = None;
    for attempt in 0..=retries {
        match client.get(url.clone()).send().await {
            Ok(response) => {
                let status = response.status();
                let retry_after = response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(cooldown_secs);
                let body = response.text().await?;
                if looks_like_verification(&body) {
                    return Err(AppError::Verification);
                }
                if status.is_success() {
                    if looks_like_client_rendered_shell(&body) {
                        return Err(AppError::ClientRenderedPage);
                    }
                    return Ok(body);
                }
                if matches!(
                    status,
                    StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
                ) {
                    return Err(AppError::RateLimited {
                        status: status.as_u16(),
                        retry_after_secs: retry_after,
                    });
                }
                last = Some(format!("HTTP {status}"));
            }
            Err(error) => last = Some(error.to_string()),
        }
        if attempt < retries {
            tokio::time::sleep(Duration::from_millis(
                300 * 2u64.pow(attempt.min(6)) + u64::from(attempt) * 73,
            ))
            .await;
        }
    }
    Err(AppError::PageAccess(
        last.unwrap_or_else(|| "未知错误".into()),
    ))
}

async fn scan_pages(
    client: &Client,
    config: &Config,
    id: u64,
    first: String,
    pages: usize,
    cancel: &CancellationToken,
) -> Result<Vec<Vec<ImageRecord>>> {
    let bar = ProgressBar::new(pages as u64);
    bar.set_style(
        ProgressStyle::with_template(
            "扫描页面 [{bar:40.cyan/blue}] {pos}/{len}，已发现 {msg}，并发 {prefix}",
        )
        .unwrap()
        .progress_chars("=>-"),
    );
    let first_records = parse_page(&first, 1);
    let mut discovered = first_records.len();
    let mut records = vec![first_records];
    bar.inc(1);
    bar.set_message(discovered.to_string());
    let mut next_page = 2usize;
    let mut effective = if config.auto_concurrency {
        1
    } else {
        config.page_concurrency
    };
    while next_page <= pages && !cancel.is_cancelled() {
        let end = (next_page + effective).min(pages + 1);
        bar.set_prefix(effective.to_string());
        let batch = stream::iter(next_page..end)
            .map(|page| {
                let client = client.clone();
                async move {
                    let html = fetch_page(
                        &client,
                        id,
                        page,
                        config.only_author,
                        config.retries,
                        config.cooldown_secs,
                    )
                    .await?;
                    Ok::<_, AppError>(parse_page(&html, page))
                }
            })
            .buffer_unordered(effective)
            .collect::<Vec<_>>()
            .await;
        for result in batch {
            let page_records = result?;
            discovered += page_records.len();
            records.push(page_records);
            bar.inc(1);
            bar.set_message(discovered.to_string());
        }
        next_page = end;
        if config.auto_concurrency && effective < config.page_concurrency {
            effective = (effective * 2).min(config.page_concurrency);
            tokio::time::sleep(Duration::from_millis(config.warmup_delay_ms)).await;
        }
    }
    bar.finish_and_clear();
    Ok(records)
}

async fn download_with_retry(
    client: &Client,
    record: &ImageRecord,
    output: &std::path::Path,
    retries: u32,
) -> (Result<DownloadOutcome>, u32) {
    for attempt in 0..=retries {
        let result = download_one(client, record, output).await;
        match &result {
            Ok(_) => return (result, attempt),
            Err(error) if error.is_rate_limited() => return (result, attempt),
            Err(error) if error.is_transient() && attempt < retries => {
                tokio::time::sleep(Duration::from_millis(
                    400 * 2u64.pow(attempt.min(6)) + u64::from(attempt) * 97,
                ))
                .await;
            }
            Err(_) => return (result, attempt),
        }
    }
    unreachable!("重试循环至少执行一次")
}

#[derive(Clone)]
struct WorkItem {
    record: ImageRecord,
    rate_retries: u32,
}

async fn download_all(
    client: &Client,
    config: &Config,
    records: Vec<ImageRecord>,
    state: Arc<Mutex<DownloadState>>,
    cancel: &CancellationToken,
) -> Result<(usize, usize, Vec<serde_json::Value>)> {
    let bar = ProgressBar::new(records.len() as u64);
    bar.set_style(
        ProgressStyle::with_template(
            "下载 [{bar:40.green/blue}] {pos}/{len} {per_sec} ETA {eta} {msg}",
        )
        .unwrap()
        .progress_chars("=>-"),
    );
    let mut queue: VecDeque<_> = records
        .into_iter()
        .map(|record| WorkItem {
            record,
            rate_retries: 0,
        })
        .collect();
    let mut effective = if config.auto_concurrency {
        config.image_concurrency.min(4)
    } else {
        config.image_concurrency
    };
    let mut succeeded = 0usize;
    let mut skipped = 0usize;
    let mut failed = Vec::new();
    while !queue.is_empty() && !cancel.is_cancelled() {
        let batch_len = effective.min(queue.len());
        let batch: Vec<_> = (0..batch_len).filter_map(|_| queue.pop_front()).collect();
        let output = config.output_dir.clone();
        let results = stream::iter(batch)
            .map(|work| {
                let client = client.clone();
                let output = output.clone();
                async move {
                    let (result, retries) =
                        download_with_retry(&client, &work.record, &output, config.retries).await;
                    (work, result, retries)
                }
            })
            .buffer_unordered(effective)
            .collect::<Vec<_>>()
            .await;
        let mut limited = 0usize;
        let mut cooldown = config.cooldown_secs;
        for (mut work, result, retries) in results {
            let file = work.record.target_file.clone();
            let mut guard = state.lock().await;
            let item = guard.entry(file.clone()).or_default();
            item.retries = item.retries.saturating_add(retries);
            item.updated_at = chrono::Utc::now();
            match result {
                Ok(DownloadOutcome::Completed { bytes, .. }) => {
                    item.status = ItemStatus::Completed;
                    item.downloaded_bytes = bytes;
                    succeeded += 1;
                    bar.inc(1);
                }
                Ok(DownloadOutcome::Skipped { .. }) => {
                    item.status = ItemStatus::Completed;
                    skipped += 1;
                    bar.inc(1);
                }
                Err(error) if error.is_rate_limited() && work.rate_retries < config.retries => {
                    limited += 1;
                    cooldown =
                        cooldown.max(error.retry_after_secs().unwrap_or(config.cooldown_secs));
                    work.rate_retries += 1;
                    item.status = ItemStatus::Pending;
                    item.retries = item.retries.saturating_add(1);
                    item.last_error = Some(error.to_string());
                    queue.push_back(work);
                }
                Err(error) => {
                    item.status = ItemStatus::Failed;
                    item.last_error = Some(error.to_string());
                    failed.push(serde_json::json!({"file": file, "url": work.record.normalized_url, "error": error.to_string()}));
                    bar.inc(1);
                }
            }
        }
        if limited > 0 {
            effective = if config.auto_concurrency {
                adaptive::decrease(effective)
            } else {
                effective
            };
            bar.set_message(format!("成功 {succeeded} 跳过 {skipped} 失败 {}，限流 {limited}，冷却 {cooldown}s，并发 {effective}", failed.len()));
            tokio::select! { _ = tokio::time::sleep(Duration::from_secs(cooldown)) => {}, _ = cancel.cancelled() => {} }
        } else {
            if config.auto_concurrency && effective < config.image_concurrency {
                effective = adaptive::increase(effective, config.image_concurrency);
                tokio::time::sleep(Duration::from_millis(config.warmup_delay_ms)).await;
            }
            bar.set_message(format!(
                "成功 {succeeded} 跳过 {skipped} 失败 {}，并发 {effective}",
                failed.len()
            ));
        }
    }
    bar.finish_and_clear();
    Ok((succeeded, skipped, failed))
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("错误：{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let config = cli::collect()?;
    let id = extract_thread_id(&config.thread_url)?;
    tokio::fs::create_dir_all(&config.output_dir).await?;
    if config.clear_login {
        keychain::clear()?;
        println!("已清除 macOS 钥匙串中的贴吧会话。");
    }
    let cookie = match &config.cookie_file {
        Some(path) => {
            let value = load_cookie(path).await?;
            println!("已加载 Cookie 文件（内容不会写入日志或任务状态）");
            Some(value)
        }
        None => {
            let saved = keychain::load()?;
            if saved.is_some() {
                println!("已从 macOS 钥匙串加载贴吧会话。");
            }
            saved
        }
    };
    let mut client = build_client(cookie)?;
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });

    println!("正在以单请求预检帖子访问权限...");
    let first_attempt = fetch_page(
        &client,
        id,
        1,
        config.only_author,
        config.retries,
        config.cooldown_secs,
    )
    .await;
    let mut browser_api_pages = None;
    let first = match first_attempt {
        Ok(html) => html,
        Err(error) if config.browser_login && error.requires_browser_login() => {
            println!("检测到百度安全验证，即将打开专用 Chrome 窗口。");
            println!("请在窗口内正常登录并完成验证；程序会自动检测成功，无需打开开发者工具。");
            let browser_result = chrome_auth::login(
                &config.thread_url,
                config.only_author,
                config.chrome_path.as_deref(),
                Duration::from_secs(config.login_timeout_secs),
            )
            .await?;
            if config.remember_login {
                keychain::save(&browser_result.cookie)?;
                println!("会话已安全保存到 macOS 钥匙串。");
            }
            client = build_client(Some(browser_result.cookie))?;
            if let Some(directory) = &config.diagnostic_html_dir {
                tokio::fs::create_dir_all(directory).await?;
                tokio::fs::write(
                    directory.join("browser-page-1.html"),
                    browser_result.rendered_html.as_bytes(),
                )
                .await?;
                atomic_write_json(
                    &directory.join("browser-resources.json"),
                    &browser_result.resources,
                )
                .await?;
                for (index, request) in browser_result.page_api_requests.iter().enumerate() {
                    tokio::fs::write(
                        directory.join(format!("page-api-request-{}.txt", index + 1)),
                        request,
                    )
                    .await?;
                }
                for (index, response) in browser_result.page_api_responses.iter().enumerate() {
                    tokio::fs::write(
                        directory.join(format!("page-api-response-{}.json", index + 1)),
                        response,
                    )
                    .await?;
                }
            }
            println!("浏览器验证和渲染完成，正在解析页面...");
            browser_api_pages = Some(browser_result.page_api_responses);
            browser_result.rendered_html
        }
        Err(error) => return Err(error),
    };
    let pages = match browser_api_pages.as_ref().and_then(|v| v.first()) {
        Some(body) => api_total_pages(body)?,
        None => total_pages(&first),
    };
    if let Some(directory) = &config.diagnostic_html_dir {
        tokio::fs::create_dir_all(directory).await?;
        tokio::fs::write(directory.join("page-1.html"), first.as_bytes()).await?;
        println!("诊断 HTML 已保存到 {}", directory.display());
    }
    println!(
        "预检通过：帖子 {id}，共 {pages} 页；模式：{}",
        if config.only_author {
            "只看楼主"
        } else {
            "全帖"
        }
    );
    let page_records = if let Some(api_pages) = browser_api_pages {
        if api_pages.len() != pages {
            return Err(AppError::PageAccess(format!(
                "浏览器仅捕获到 {}/{} 页 API 响应",
                api_pages.len(),
                pages
            )));
        }
        api_pages
            .iter()
            .enumerate()
            .map(|(index, body)| parse_api_page(body, index + 1))
            .collect::<Result<Vec<_>>>()?
    } else {
        scan_pages(&client, &config, id, first, pages, &cancel).await?
    };
    let records = sort_deduplicate(page_records);
    atomic_write_json(&config.output_dir.join("manifest.json"), &records).await?;
    let state: Arc<Mutex<DownloadState>> = Arc::new(Mutex::new(
        records
            .iter()
            .map(|r| (r.target_file.clone(), ItemState::default()))
            .collect(),
    ));
    atomic_write_json(
        &config.output_dir.join("download-state.json"),
        &*state.lock().await,
    )
    .await?;
    println!(
        "发现 {} 张正文原图；最大下载并发 {}，自动调节 {}",
        records.len(),
        config.image_concurrency,
        if config.auto_concurrency {
            "开启"
        } else {
            "关闭"
        }
    );
    let (succeeded, skipped, failed) =
        download_all(&client, &config, records, state.clone(), &cancel).await?;
    atomic_write_json(
        &config.output_dir.join("download-state.json"),
        &*state.lock().await,
    )
    .await?;
    atomic_write_json(&config.output_dir.join("failed.json"), &failed).await?;
    println!(
        "完成：成功 {succeeded}，跳过 {skipped}，失败 {}。状态目录：{}",
        failed.len(),
        config.output_dir.display()
    );
    if cancel.is_cancelled() {
        println!("已安全停止；.part 文件和任务状态已保留。");
    }
    Ok(())
}
