use crate::{AppError, Result};
use url::Url;

pub fn extract_thread_id(input: &str) -> Result<u64> {
    let value = input.trim();
    let with_scheme = if value.starts_with("http://") || value.starts_with("https://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| AppError::InvalidThreadUrl(value.into()))?;
    if !matches!(
        url.host_str(),
        Some("tieba.baidu.com") | Some("www.tieba.baidu.com")
    ) {
        return Err(AppError::InvalidThreadUrl(value.into()));
    }
    let mut segments = url
        .path_segments()
        .ok_or_else(|| AppError::InvalidThreadUrl(value.into()))?;
    if segments.next() != Some("p") {
        return Err(AppError::InvalidThreadUrl(value.into()));
    }
    let id = segments
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| AppError::InvalidThreadUrl(value.into()))?;
    if segments.next().is_some() {
        return Err(AppError::InvalidThreadUrl(value.into()));
    }
    Ok(id)
}

pub fn page_url(thread_id: u64, page: usize, only_author: bool) -> Url {
    let mut url = Url::parse(&format!("https://tieba.baidu.com/p/{thread_id}")).unwrap();
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("pn", &page.to_string());
        if only_author {
            query.append_pair("see_lz", "1");
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_supported_urls() {
        for input in [
            "https://tieba.baidu.com/p/123456789",
            "https://tieba.baidu.com/p/123456789?pn=2",
            "http://tieba.baidu.com/p/123456789?see_lz=1",
            "tieba.baidu.com/p/123456789",
        ] {
            assert_eq!(extract_thread_id(input).unwrap(), 123456789);
        }
    }

    #[test]
    fn rejects_invalid_urls() {
        for input in [
            "",
            "https://example.com/p/1",
            "tieba.baidu.com/f?kw=x",
            "tieba.baidu.com/p/no",
        ] {
            assert!(extract_thread_id(input).is_err(), "{input}");
        }
    }

    #[test]
    fn constructs_modes() {
        assert_eq!(
            page_url(42, 3, false).as_str(),
            "https://tieba.baidu.com/p/42?pn=3"
        );
        assert_eq!(
            page_url(42, 3, true).as_str(),
            "https://tieba.baidu.com/p/42?pn=3&see_lz=1"
        );
    }
}
