use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("无效的贴吧帖子链接：{0}")]
    InvalidThreadUrl(String),
    #[error("无法读取 Cookie 文件 {path}：{source}")]
    CookieRead {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Cookie 文件格式无效：{0}")]
    InvalidCookie(String),
    #[error("Chrome 登录失败：{0}")]
    BrowserLogin(String),
    #[error("贴吧返回了需要浏览器渲染的页面")]
    ClientRenderedPage,
    #[error("页面访问失败：{0}")]
    PageAccess(String),
    #[error("检测到登录页、安全验证或验证码，任务已停止；请稍后重试或提供 Cookie 文件")]
    Verification,
    #[error("服务器限流或拒绝访问：HTTP {status}，建议等待 {retry_after_secs} 秒")]
    RateLimited { status: u16, retry_after_secs: u64 },
    #[error("响应不是图片：{0}")]
    NotImage(String),
    #[error("断点续传响应不一致：{0}")]
    InvalidRange(String),
    #[error("状态文件损坏 {path}：{source}")]
    StateCorrupt {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Verification)
    }

    pub fn requires_browser_login(&self) -> bool {
        matches!(
            self,
            Self::Verification | Self::ClientRenderedPage | Self::RateLimited { status: 403, .. }
        )
    }

    pub fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::RateLimited {
                retry_after_secs, ..
            } => Some(*retry_after_secs),
            Self::Verification => None,
            _ => None,
        }
    }

    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Http(_) | Self::PageAccess(_) | Self::RateLimited { .. }
        )
    }
}
