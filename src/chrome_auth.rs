use crate::{AppError, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tokio::{fs, net::TcpStream};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
struct Target {
    #[serde(rename = "type")]
    kind: String,
    url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    websocket_url: Option<String>,
}

pub struct BrowserLoginResult {
    pub cookie: String,
    pub rendered_html: String,
    pub resources: Vec<String>,
    pub page_api_requests: Vec<String>,
    pub page_api_responses: Vec<String>,
}

async fn capture_page_api(
    socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    command_id: &mut u64,
    navigate_to: &str,
) -> Result<Option<(String, String)>> {
    cdp_command(
        socket,
        *command_id,
        "Page.navigate",
        json!({"url": navigate_to}),
    )
    .await?;
    *command_id += 1;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut post_data = None;
    let mut target_request_id = None;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        let Some(message) = tokio::time::timeout(remaining, socket.next())
            .await
            .map_err(|_| AppError::BrowserLogin("等待贴吧页面接口超时".into()))?
        else {
            return Ok(None);
        };
        let message = message.map_err(|error| AppError::BrowserLogin(error.to_string()))?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text)?;
        let method = value.get("method").and_then(Value::as_str);
        if method == Some("Network.loadingFinished") {
            let request_id = value.pointer("/params/requestId").and_then(Value::as_str);
            if request_id == target_request_id.as_deref() {
                let request_id = request_id.unwrap_or_default();
                let body = cdp_command(
                    socket,
                    *command_id,
                    "Network.getResponseBody",
                    json!({"requestId": request_id}),
                )
                .await?;
                *command_id += 1;
                if let Some(body) = body.get("body").and_then(Value::as_str) {
                    return Ok(Some((post_data.unwrap_or_default(), body.to_owned())));
                }
            }
            continue;
        }
        let url = value
            .pointer("/params/request/url")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .pointer("/params/response/url")
                    .and_then(Value::as_str)
            });
        if !url.is_some_and(|url| url.contains("/c/f/pb/page_pc")) {
            continue;
        }
        if method == Some("Network.requestWillBeSent") {
            post_data = value
                .pointer("/params/request/postData")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            continue;
        }
        if method == Some("Network.responseReceived") {
            let Some(request_id) = value.pointer("/params/requestId").and_then(Value::as_str)
            else {
                continue;
            };
            if post_data.is_none() {
                let request = cdp_command(
                    socket,
                    *command_id,
                    "Network.getRequestPostData",
                    json!({"requestId": request_id}),
                )
                .await?;
                *command_id += 1;
                post_data = request
                    .get("postData")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
            target_request_id = Some(request_id.to_owned());
        }
    }
}

pub fn find_chrome(custom: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = custom {
        if path.is_file() {
            return Ok(path.to_owned());
        }
        return Err(AppError::BrowserLogin(format!(
            "指定的 Chrome 不存在：{}",
            path.display()
        )));
    }
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or_else(|| AppError::BrowserLogin("未找到 Chrome、Chromium 或 Brave".into()))
}

pub fn default_profile_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Application Support/tieba-image-downloader/chrome-profile")
}

async fn wait_for_debug_port(profile: &Path, timeout: Duration) -> Result<u16> {
    let file = profile.join("DevToolsActivePort");
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(content) = fs::read_to_string(&file).await
            && let Some(port) = content
                .lines()
                .next()
                .and_then(|line| line.parse::<u16>().ok())
        {
            return Ok(port);
        }
        if Instant::now() >= deadline {
            return Err(AppError::BrowserLogin(
                "Chrome 启动超时，未发现本地调试端口".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn find_page_target(port: u16, thread_url: &str) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let endpoint = format!("http://127.0.0.1:{port}/json/list");
    loop {
        if let Ok(response) = reqwest::get(&endpoint).await
            && let Ok(targets) = response.json::<Vec<Target>>().await
            && let Some(url) = targets
                .into_iter()
                .find(|target| {
                    target.kind == "page"
                        && (target.url.contains("tieba.baidu.com") || target.url == thread_url)
                })
                .and_then(|target| target.websocket_url)
        {
            return Ok(url);
        }
        if Instant::now() >= deadline {
            return Err(AppError::BrowserLogin(
                "无法连接专用 Chrome 中的贴吧页面".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn cdp_command(
    socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value> {
    socket
        .send(Message::Text(
            json!({"id": id, "method": method, "params": params})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|error| AppError::BrowserLogin(error.to_string()))?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AppError::BrowserLogin(format!("Chrome 命令超时：{method}")));
        }
        let message = tokio::time::timeout(remaining, socket.next())
            .await
            .map_err(|_| AppError::BrowserLogin(format!("Chrome 命令超时：{method}")))?
            .ok_or_else(|| AppError::BrowserLogin("Chrome 调试连接意外关闭".into()))?;
        let message = message.map_err(|error| AppError::BrowserLogin(error.to_string()))?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text)?;
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            if let Some(error) = value.get("error") {
                return Err(AppError::BrowserLogin(format!("CDP {method}: {error}")));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

fn cookie_header(result: &Value) -> Option<String> {
    let mut cookies: Vec<(String, String)> = result
        .get("cookies")?
        .as_array()?
        .iter()
        .filter(|cookie| {
            cookie
                .get("domain")
                .and_then(Value::as_str)
                .is_some_and(|domain| {
                    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
                    domain == "baidu.com" || domain.ends_with(".baidu.com")
                })
        })
        .filter_map(|cookie| {
            Some((
                cookie.get("name")?.as_str()?.to_owned(),
                cookie.get("value")?.as_str()?.to_owned(),
            ))
        })
        .collect();
    cookies.sort_by(|left, right| left.0.cmp(&right.0));
    cookies.dedup_by(|left, right| left.0 == right.0);
    (!cookies.is_empty()).then(|| {
        cookies
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

pub async fn login(
    thread_url: &str,
    only_author: bool,
    chrome_path: Option<&Path>,
    timeout: Duration,
) -> Result<BrowserLoginResult> {
    let executable = find_chrome(chrome_path)?;
    let profile = default_profile_dir();
    fs::create_dir_all(&profile).await?;
    let _ = fs::remove_file(profile.join("DevToolsActivePort")).await;
    Command::new(&executable)
        .arg("--remote-debugging-address=127.0.0.1")
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--new-window")
        .arg(thread_url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| AppError::BrowserLogin(format!("无法启动 Chrome：{error}")))?;

    let port = wait_for_debug_port(&profile, Duration::from_secs(15)).await?;
    let websocket_url = find_page_target(port, thread_url).await?;
    let (mut socket, _) = connect_async(&websocket_url)
        .await
        .map_err(|error| AppError::BrowserLogin(error.to_string()))?;
    let _ = cdp_command(&mut socket, 1, "Network.enable", json!({})).await?;
    let deadline = Instant::now() + timeout;
    let mut command_id = 2u64;
    loop {
        let state = cdp_command(
            &mut socket,
            command_id,
            "Runtime.evaluate",
            json!({"expression": "({title: document.title, url: location.href, ready: document.readyState})", "returnByValue": true}),
        )
        .await?;
        command_id += 1;
        let page = state
            .pointer("/result/value")
            .cloned()
            .unwrap_or(Value::Null);
        let title = page
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let url = page.get("url").and_then(Value::as_str).unwrap_or_default();
        if url.contains("tieba.baidu.com/p/")
            && !title.is_empty()
            && !title.contains("安全验证")
            && !title.contains("登录")
        {
            let cookies = cdp_command(
                &mut socket,
                command_id,
                "Network.getCookies",
                json!({"urls": [thread_url, "https://tieba.baidu.com/"]}),
            )
            .await?;
            if let Some(header) = cookie_header(&cookies) {
                command_id += 1;
                let base_url = thread_url.split('?').next().unwrap_or(thread_url);
                let first_url = format!("{base_url}?pn=1&lz={}", usize::from(only_author));
                let first_capture =
                    capture_page_api(&mut socket, &mut command_id, &first_url).await?;
                let total_pages = first_capture
                    .as_ref()
                    .and_then(|(_, body)| serde_json::from_str::<Value>(body).ok())
                    .and_then(|value| value.pointer("/page/total_page").and_then(Value::as_u64))
                    .unwrap_or(1) as usize;
                let mut page_api_requests = Vec::with_capacity(total_pages);
                let mut page_api_responses = Vec::with_capacity(total_pages);
                if let Some((request, response)) = first_capture {
                    page_api_requests.push(request);
                    page_api_responses.push(response);
                }
                for page in 2..=total_pages {
                    let url = format!("{base_url}?pn={page}&lz={}", usize::from(only_author));
                    if let Some((request, response)) =
                        capture_page_api(&mut socket, &mut command_id, &url).await?
                    {
                        page_api_requests.push(request);
                        page_api_responses.push(response);
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                command_id += 1;
                let html = cdp_command(
                    &mut socket,
                    command_id,
                    "Runtime.evaluate",
                    json!({"expression": "document.documentElement.outerHTML", "returnByValue": true}),
                )
                .await?;
                let rendered_html = html
                    .pointer("/result/value")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                command_id += 1;
                let resource_result = cdp_command(
                    &mut socket,
                    command_id,
                    "Runtime.evaluate",
                    json!({"expression": "performance.getEntriesByType('resource').map(e => e.name)", "returnByValue": true}),
                )
                .await?;
                let resources = resource_result
                    .pointer("/result/value")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect();
                let _ = cdp_command(&mut socket, command_id + 1, "Browser.close", json!({})).await;
                return Ok(BrowserLoginResult {
                    cookie: header,
                    rendered_html,
                    resources,
                    page_api_requests,
                    page_api_responses,
                });
            }
        }
        if Instant::now() >= deadline {
            return Err(AppError::BrowserLogin(
                "等待登录或安全验证超时；专用浏览器会话已保留，可再次尝试".into(),
            ));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_baidu_cookies_without_values_in_debug_output() {
        let value = json!({"cookies": [
            {"domain": ".baidu.com", "name": "BDUSS", "value": "secret"},
            {"domain": "tieba.baidu.com", "name": "BAIDUID", "value": "id"},
            {"domain": ".example.com", "name": "BAD", "value": "ignored"}
        ]});
        assert_eq!(cookie_header(&value).unwrap(), "BAIDUID=id; BDUSS=secret");
    }

    #[test]
    fn custom_chrome_path_must_exist() {
        assert!(find_chrome(Some(Path::new("/definitely/missing/chrome"))).is_err());
    }
}
