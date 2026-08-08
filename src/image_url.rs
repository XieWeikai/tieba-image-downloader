use url::Url;

pub fn absolute_url(value: &str) -> Option<Url> {
    let decoded = value.replace("w%3D580", "w=580");
    let full = if decoded.starts_with("//") {
        format!("https:{decoded}")
    } else if decoded.starts_with("http://") || decoded.starts_with("https://") {
        decoded
    } else {
        return None;
    };
    Url::parse(&full).ok()
}

pub fn original_url(value: &str) -> Option<Url> {
    let mut url = absolute_url(value)?;
    if url.scheme() == "http" {
        let _ = url.set_scheme("https");
    }
    let path = url.path().to_owned();
    if let Some(pos) = path.find("/forum/w=580/") {
        url.set_path(&format!(
            "{}/forum/pic/item/{}",
            &path[..pos],
            &path[pos + 13..]
        ));
    }
    url.set_fragment(None);
    Some(url)
}

pub fn normalized_key(url: &Url) -> String {
    let mut value = url.clone();
    value.set_fragment(None);
    value.to_string()
}

pub fn extension_from_content_type(value: Option<&str>) -> Option<&'static str> {
    let media = value?.split(';').next()?.trim().to_ascii_lowercase();
    match media.as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        "image/bmp" => Some("bmp"),
        "image/avif" => Some("avif"),
        _ => None,
    }
}

pub fn extension_from_url(url: &Url) -> Option<&str> {
    let segment = url.path_segments()?.next_back()?;
    let ext = segment.rsplit_once('.')?.1.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some("jpg"),
        "png" => Some("png"),
        "webp" => Some("webp"),
        "gif" => Some("gif"),
        "bmp" => Some("bmp"),
        "avif" => Some("avif"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_protocol_and_thumbnail() {
        assert_eq!(
            original_url("//tiebapic.baidu.com/forum/w=580/a.jpg")
                .unwrap()
                .as_str(),
            "https://tiebapic.baidu.com/forum/pic/item/a.jpg"
        );
        assert_eq!(
            original_url("http://imgsa.baidu.com/forum/pic/item/a.png?q=1")
                .unwrap()
                .scheme(),
            "https"
        );
    }
    #[test]
    fn maps_extensions() {
        assert_eq!(
            extension_from_content_type(Some("image/jpeg; charset=x")),
            Some("jpg")
        );
        assert_eq!(extension_from_content_type(Some("text/html")), None);
        assert_eq!(
            extension_from_url(&Url::parse("https://x/a.webp?q=1").unwrap()),
            Some("webp")
        );
    }
}
