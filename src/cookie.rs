use crate::{AppError, Result};
use std::{collections::BTreeMap, path::Path};
use tokio::fs;

fn is_baidu_domain(domain: &str) -> bool {
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    domain == "baidu.com" || domain.ends_with(".baidu.com")
}

pub fn parse_cookie_file(content: &str) -> Result<String> {
    let is_netscape = content.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with('#') && line.split('\t').count() >= 7
    });
    if !is_netscape {
        let raw = content.trim();
        if raw.is_empty() || !raw.contains('=') || raw.contains(['\r', '\n']) {
            return Err(AppError::InvalidCookie(
                "完整 Cookie 请求头必须是单行 name=value; name2=value2".into(),
            ));
        }
        return Ok(raw.to_owned());
    }

    let now = chrono::Utc::now().timestamp();
    let mut values = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() < 7 || !is_baidu_domain(fields[0]) || !fields[2].starts_with('/') {
            continue;
        }
        let expires = fields[4].parse::<i64>().unwrap_or(0);
        if expires != 0 && expires <= now {
            continue;
        }
        let name = fields[5].trim();
        let value = fields[6].trim();
        if !name.is_empty() && !value.is_empty() {
            values.insert(name.to_owned(), value.to_owned());
        }
    }
    if values.is_empty() {
        return Err(AppError::InvalidCookie(
            "Netscape cookies.txt 中没有可用于 baidu.com 的未过期 Cookie".into(),
        ));
    }
    Ok(values
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; "))
}

pub async fn load_cookie(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)
        .await
        .map_err(|source| AppError::CookieRead {
            path: path.to_owned(),
            source,
        })?;
    parse_cookie_file(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_raw_header() {
        assert_eq!(
            parse_cookie_file("BAIDUID=a; BDUSS=b").unwrap(),
            "BAIDUID=a; BDUSS=b"
        );
    }

    #[test]
    fn imports_netscape_and_filters_domains() {
        let content = "# Netscape HTTP Cookie File\n.baidu.com\tTRUE\t/\tTRUE\t0\tBDUSS\tsecret\n.example.com\tTRUE\t/\tFALSE\t0\tBAD\tx\ntieba.baidu.com\tFALSE\t/\tFALSE\t0\tBAIDUID\tid\n";
        assert_eq!(
            parse_cookie_file(content).unwrap(),
            "BAIDUID=id; BDUSS=secret"
        );
    }

    #[test]
    fn rejects_empty_or_unusable_files() {
        assert!(parse_cookie_file("").is_err());
        assert!(parse_cookie_file("# Netscape HTTP Cookie File\n").is_err());
    }
}
