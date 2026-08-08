use crate::{AppError, Result};

const SERVICE: &str = "tieba-image-downloader";
const ACCOUNT: &str = "tieba.baidu.com-session";

#[cfg(target_os = "macos")]
pub fn load() -> Result<Option<String>> {
    match security_framework::passwords::get_generic_password(SERVICE, ACCOUNT) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| AppError::InvalidCookie("钥匙串中的会话不是有效 UTF-8".into())),
        Err(error) if error.code() == -25300 => Ok(None),
        Err(error) => Err(AppError::BrowserLogin(format!(
            "无法读取 macOS 钥匙串：{error}"
        ))),
    }
}

#[cfg(target_os = "macos")]
pub fn save(cookie: &str) -> Result<()> {
    security_framework::passwords::set_generic_password(SERVICE, ACCOUNT, cookie.as_bytes())
        .map_err(|error| AppError::BrowserLogin(format!("无法保存会话到 macOS 钥匙串：{error}")))
}

#[cfg(target_os = "macos")]
pub fn clear() -> Result<()> {
    match security_framework::passwords::delete_generic_password(SERVICE, ACCOUNT) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == -25300 => Ok(()),
        Err(error) => Err(AppError::BrowserLogin(format!(
            "无法删除 macOS 钥匙串会话：{error}"
        ))),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn load() -> Result<Option<String>> {
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
pub fn save(_cookie: &str) -> Result<()> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn clear() -> Result<()> {
    Ok(())
}
