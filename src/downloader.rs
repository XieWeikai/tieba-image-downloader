use crate::{AppError, Result, image_url::extension_from_content_type, parser::ImageRecord};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadOutcome {
    Completed { bytes: u64, path: PathBuf },
    Skipped { path: PathBuf },
}

fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let value = value.strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    Some((
        start.parse().ok()?,
        end.parse().ok()?,
        (total != "*").then(|| total.parse().ok()).flatten(),
    ))
}

fn response_is_image(content_type: Option<&str>) -> bool {
    extension_from_content_type(content_type).is_some()
}

pub async fn download_one(
    client: &Client,
    record: &ImageRecord,
    output: &Path,
) -> Result<DownloadOutcome> {
    let intended = output.join(&record.target_file);
    if let Ok(meta) = fs::metadata(&intended).await
        && meta.len() > 0
    {
        return Ok(DownloadOutcome::Skipped { path: intended });
    }
    let part = intended.with_extension(format!(
        "{}.part",
        intended
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("jpg")
    ));
    let existing = fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    let mut request = client
        .get(&record.normalized_url)
        .header(header::REFERER, "https://tieba.baidu.com/");
    if existing > 0 {
        request = request.header(header::RANGE, format!("bytes={existing}-"));
    }
    let response = request.send().await?;
    if matches!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS | StatusCode::FORBIDDEN
    ) {
        let retry_after_secs = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30);
        return Err(AppError::RateLimited {
            status: response.status().as_u16(),
            retry_after_secs,
        });
    }
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        if let Some(total) = response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("bytes */"))
            .and_then(|v| v.parse::<u64>().ok())
            && total == existing
        {
            fs::rename(&part, &intended).await?;
            return Ok(DownloadOutcome::Completed {
                bytes: existing,
                path: intended,
            });
        }
        let _ = fs::remove_file(&part).await;
        return Err(AppError::InvalidRange(
            "服务器返回 416，且本地大小无法确认完整".into(),
        ));
    }
    if !response.status().is_success() {
        return Err(AppError::PageAccess(format!(
            "图片 HTTP {}",
            response.status()
        )));
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);
    if !response_is_image(content_type.as_deref()) {
        return Err(AppError::NotImage(
            content_type.unwrap_or_else(|| "缺少 Content-Type".into()),
        ));
    }
    let append = if existing > 0 && response.status() == StatusCode::PARTIAL_CONTENT {
        let range = response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range)
            .ok_or_else(|| AppError::InvalidRange("206 缺少有效 Content-Range".into()))?;
        if range.0 != existing {
            return Err(AppError::InvalidRange(format!(
                "期望从 {existing} 开始，实际为 {}",
                range.0
            )));
        }
        true
    } else {
        false
    };
    let mut options = fs::OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let mut file = options.open(&part).await?;
    let mut stream = response.bytes_stream();
    let mut written = if append { existing } else { 0 };
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if chunk.is_empty() {
            continue;
        }
        file.write_all(&chunk).await?;
        written += chunk.len() as u64;
    }
    file.flush().await?;
    drop(file);
    if written == 0 {
        return Err(AppError::NotImage("响应体为空".into()));
    }
    let final_url = Url::parse(&record.normalized_url)?;
    let ext = extension_from_content_type(content_type.as_deref())
        .or_else(|| crate::image_url::extension_from_url(&final_url))
        .unwrap_or("jpg");
    let final_path = intended.with_extension(ext);
    fs::rename(&part, &final_path).await?;
    Ok(DownloadOutcome::Completed {
        bytes: written,
        path: final_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header as header_match, method, path},
    };
    fn record(url: String) -> ImageRecord {
        ImageRecord {
            page: 1,
            post_order: 0,
            image_order: 0,
            floor: Some(1),
            author: None,
            post_id: None,
            original_url: url.clone(),
            normalized_url: url,
            target_file: "00001_f0001_hash.jpg".into(),
        }
    }
    #[tokio::test]
    async fn downloads_and_skips_complete_file() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "image/png")
                    .set_body_bytes(b"PNG".to_vec()),
            )
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let out = download_one(
            &Client::new(),
            &record(format!("{}/x", server.uri())),
            dir.path(),
        )
        .await
        .unwrap();
        let path = match out {
            DownloadOutcome::Completed { path, .. } => path,
            _ => panic!(),
        };
        assert_eq!(path.extension().unwrap(), "png");
        assert_eq!(fs::read(path).await.unwrap(), b"PNG");
    }
    #[tokio::test]
    async fn resumes_206() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/x"))
            .and(header_match("range", "bytes=3-"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("Content-Type", "image/jpeg")
                    .insert_header("Content-Range", "bytes 3-5/6")
                    .set_body_bytes(b"def".to_vec()),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("00001_f0001_hash.jpg.part"), b"abc")
            .await
            .unwrap();
        download_one(
            &Client::new(),
            &record(format!("{}/x", server.uri())),
            dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(
            fs::read(dir.path().join("00001_f0001_hash.jpg"))
                .await
                .unwrap(),
            b"abcdef"
        );
    }
    #[tokio::test]
    async fn overwrites_when_range_ignored() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "image/jpeg")
                    .set_body_bytes(b"new".to_vec()),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("00001_f0001_hash.jpg.part"), b"old-part")
            .await
            .unwrap();
        download_one(
            &Client::new(),
            &record(format!("{}/x", server.uri())),
            dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(
            fs::read(dir.path().join("00001_f0001_hash.jpg"))
                .await
                .unwrap(),
            b"new"
        );
    }
    #[tokio::test]
    async fn accepts_complete_416() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(416).insert_header("Content-Range", "bytes */3"))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("00001_f0001_hash.jpg.part"), b"abc")
            .await
            .unwrap();
        download_one(
            &Client::new(),
            &record(format!("{}/x", server.uri())),
            dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(
            fs::read(dir.path().join("00001_f0001_hash.jpg"))
                .await
                .unwrap(),
            b"abc"
        );
    }
    #[tokio::test]
    async fn rejects_html() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "text/html")
                    .set_body_string("captcha"),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            download_one(
                &Client::new(),
                &record(format!("{}/x", server.uri())),
                dir.path()
            )
            .await,
            Err(AppError::NotImage(_))
        ));
    }

    #[tokio::test]
    async fn reports_retry_after_on_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "17"))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let error = download_one(
            &Client::new(),
            &record(format!("{}/x", server.uri())),
            dir.path(),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            AppError::RateLimited {
                status: 429,
                retry_after_secs: 17
            }
        ));
    }
}
