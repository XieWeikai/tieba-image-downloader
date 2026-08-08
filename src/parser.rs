use crate::{
    AppError, Result,
    image_url::{normalized_key, original_url},
};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageRecord {
    pub page: usize,
    pub post_order: usize,
    pub image_order: usize,
    pub floor: Option<u64>,
    pub author: Option<String>,
    pub post_id: Option<String>,
    pub original_url: String,
    pub normalized_url: String,
    pub target_file: String,
}

pub fn total_pages(html: &str) -> usize {
    let doc = Html::parse_document(html);
    let selectors = [
        "li.l_reply_num span.red",
        "a.last.pagination-item",
        ".l_pager a",
    ];
    let mut numbers = Vec::new();
    for raw in selectors {
        if let Ok(selector) = Selector::parse(raw) {
            if raw == "li.l_reply_num span.red" {
                if let Some(page_count) = doc
                    .select(&selector)
                    .filter_map(|node| node.text().collect::<String>().trim().parse::<usize>().ok())
                    .next_back()
                {
                    numbers.push(page_count);
                }
                continue;
            }
            for node in doc.select(&selector) {
                numbers.extend(node.text().filter_map(|v| v.trim().parse::<usize>().ok()));
                if let Some(href) = node.value().attr("href")
                    && let Ok(url) = Url::parse(&format!("https://tieba.baidu.com{href}"))
                {
                    numbers.extend(url.query_pairs().filter_map(|(k, v)| {
                        (k == "pn").then(|| v.parse::<usize>().ok()).flatten()
                    }));
                }
            }
        }
    }
    numbers.into_iter().max().unwrap_or(1).max(1)
}

fn metadata(post: &ElementRef<'_>, key: &str) -> Option<String> {
    post.value()
        .attr("data-field")
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|v| {
            v.pointer(key).and_then(|x| {
                x.as_str()
                    .map(ToOwned::to_owned)
                    .or_else(|| x.as_u64().map(|n| n.to_string()))
            })
        })
}

pub fn parse_page(html: &str, page: usize) -> Vec<ImageRecord> {
    let doc = Html::parse_document(html);
    let post_sel = Selector::parse("div.l_post, div[data-field].l_post").unwrap();
    let body_sel = Selector::parse(".d_post_content, .j_d_post_content").unwrap();
    let image_sel = Selector::parse("img").unwrap();
    let original_link_sel = Selector::parse("a[href*='/forum/pic/item/']").unwrap();
    let mut out = Vec::new();
    for (post_order, post) in doc.select(&post_sel).enumerate() {
        let author = metadata(&post, "/author/user_name");
        let post_id = metadata(&post, "/content/post_id")
            .or_else(|| post.value().attr("data-pid").map(ToOwned::to_owned));
        let floor = metadata(&post, "/content/post_no").and_then(|v| v.parse().ok());
        for body in post.select(&body_sel) {
            let links: Vec<_> = body
                .select(&original_link_sel)
                .filter_map(|a| a.value().attr("href"))
                .collect();
            for (image_order, img) in body.select(&image_sel).enumerate() {
                let classes = img.value().attr("class").unwrap_or_default();
                if classes.contains("BDE_Smiley") || classes.contains("emotion") {
                    continue;
                }
                let candidate = img
                    .value()
                    .attr("data-original")
                    .or_else(|| links.get(image_order).copied())
                    .or_else(|| img.value().attr("data-src"))
                    .or_else(|| img.value().attr("src"));
                if let Some(url) = candidate.and_then(original_url) {
                    out.push(ImageRecord {
                        page,
                        post_order,
                        image_order,
                        floor,
                        author: author.clone(),
                        post_id: post_id.clone(),
                        original_url: candidate.unwrap().to_owned(),
                        normalized_url: normalized_key(&url),
                        target_file: String::new(),
                    });
                }
            }
        }
    }
    out
}

pub fn api_total_pages(body: &str) -> Result<usize> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    if value.get("error_code").and_then(|v| v.as_i64()) != Some(0) {
        return Err(AppError::PageAccess(format!(
            "贴吧页面 API 返回错误码 {}",
            value
                .get("error_code")
                .map(ToString::to_string)
                .unwrap_or_else(|| "未知".into())
        )));
    }
    Ok(value
        .pointer("/page/total_page")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as usize)
}

pub fn parse_api_page(body: &str, page: usize) -> Result<Vec<ImageRecord>> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    if value.get("error_code").and_then(|v| v.as_i64()) != Some(0) {
        return Err(AppError::PageAccess(format!(
            "贴吧第 {page} 页 API 返回错误码 {}",
            value
                .get("error_code")
                .map(ToString::to_string)
                .unwrap_or_else(|| "未知".into())
        )));
    }
    let authors: std::collections::HashMap<u64, String> = value
        .get("user_list")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|user| {
            Some((
                user.get("id")?.as_u64()?,
                user.get("name_show")
                    .or_else(|| user.get("name"))?
                    .as_str()?
                    .to_owned(),
            ))
        })
        .collect();
    let mut posts = Vec::new();
    if page == 1
        && let Some(first) = value.get("first_floor")
    {
        posts.push(first);
    }
    posts.extend(
        value
            .get("post_list")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten(),
    );
    let mut records = Vec::new();
    for (post_order, post) in posts.into_iter().enumerate() {
        let author_id = post.get("author_id").and_then(|v| v.as_u64());
        let author = author_id.and_then(|id| authors.get(&id)).cloned();
        let floor = post.get("floor").and_then(|v| v.as_u64());
        let post_id = post.get("id").and_then(|v| {
            v.as_str()
                .map(ToOwned::to_owned)
                .or_else(|| v.as_u64().map(|id| id.to_string()))
        });
        let mut image_order = 0;
        for content in post
            .get("content")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            if content.get("type").and_then(|v| v.as_u64()) != Some(3) {
                continue;
            }
            let candidate = ["origin_src", "big_cdn_src", "cdn_src_active", "cdn_src"]
                .into_iter()
                .find_map(|key| content.get(key).and_then(|v| v.as_str()));
            if let Some(candidate) = candidate
                && let Some(url) = original_url(candidate)
            {
                records.push(ImageRecord {
                    page,
                    post_order,
                    image_order,
                    floor,
                    author: author.clone(),
                    post_id: post_id.clone(),
                    original_url: candidate.to_owned(),
                    normalized_url: normalized_key(&url),
                    target_file: String::new(),
                });
                image_order += 1;
            }
        }
    }
    Ok(records)
}

pub fn sort_deduplicate(mut pages: Vec<Vec<ImageRecord>>) -> Vec<ImageRecord> {
    let mut all: Vec<_> = pages.drain(..).flatten().collect();
    all.sort_by_key(|v| (v.page, v.post_order, v.image_order));
    let mut seen = HashSet::new();
    all.retain(|v| seen.insert(v.normalized_url.clone()));
    for (index, item) in all.iter_mut().enumerate() {
        let hash = &blake3::hash(item.normalized_url.as_bytes()).to_hex()[..8];
        let ext = Url::parse(&item.normalized_url)
            .ok()
            .and_then(|u| crate::image_url::extension_from_url(&u).map(ToOwned::to_owned))
            .unwrap_or_else(|| "jpg".into());
        item.target_file = format!(
            "{:05}_f{:04}_{}.{}",
            index + 1,
            item.floor.unwrap_or(0),
            hash,
            ext
        );
    }
    all
}

pub fn looks_like_verification(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("verify")
        || html.contains("安全验证")
        || html.contains("验证码")
        || html.contains("登录贴吧")
}

pub fn looks_like_client_rendered_shell(html: &str) -> bool {
    html.contains("<div id=\"app\"></div>")
        && (html.contains("renderType\":\"csr") || html.contains("pc-main-core"))
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../fixtures/thread-page.html");
    #[test]
    fn parses_total_pages() {
        assert_eq!(total_pages(FIXTURE), 3);
    }
    #[test]
    fn parses_body_and_priority() {
        let v = parse_page(FIXTURE, 1);
        assert_eq!(v.len(), 3);
        assert!(v[0].normalized_url.ends_with("/original.jpg"));
        assert!(v[1].normalized_url.ends_with("/lazy.png"));
        assert!(v[2].normalized_url.ends_with("/plain.webp"));
        assert_eq!(v[0].floor, Some(1));
    }
    #[test]
    fn stable_deduplication() {
        let one = parse_page(FIXTURE, 2);
        let two = parse_page(FIXTURE, 1);
        let out = sort_deduplicate(vec![one, two]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].page, 1);
        assert!(out[0].target_file.starts_with("00001_f0001_"));
    }
    #[test]
    fn detects_error_pages() {
        assert!(looks_like_verification("<title>百度安全验证</title>"));
    }
    #[test]
    fn detects_client_rendered_shell() {
        assert!(looks_like_client_rendered_shell(
            r#"<div id="app"></div><script>{"renderType":"csr"}</script>"#
        ));
    }
    #[test]
    fn parses_api_images_and_authors() {
        let body = r#"{"error_code":0,"page":{"total_page":2},"user_list":[{"id":7,"name_show":"tester"}],"first_floor":{"id":10,"floor":1,"author_id":7,"content":[{"type":3,"origin_src":"https://imgsrc.baidu.com/forum/pic/item/original.jpg","cdn_src":"https://example.com/thumb.jpg"}]},"post_list":[{"id":"11","floor":2,"author_id":7,"content":[{"type":0,"text":"x"},{"type":3,"big_cdn_src":"https://imgsrc.baidu.com/forum/pic/item/fallback.png"}]}]}"#;
        assert_eq!(api_total_pages(body).unwrap(), 2);
        let records = parse_api_page(body, 1).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].author.as_deref(), Some("tester"));
        assert!(records[0].normalized_url.ends_with("/original.jpg"));
        assert_eq!(records[1].floor, Some(2));
    }
}
