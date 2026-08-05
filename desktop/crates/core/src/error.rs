use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("HTTP请求错误: {0}")]
    Http(#[from] reqwest::Error),

    #[error("图像处理错误: {0}")]
    Image(#[from] image::ImageError),

    #[error("截屏错误: {0}")]
    Screenshot(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("AI分析错误: {0}")]
    Analysis(String),

    #[error("隐私过滤错误: {0}")]
    Privacy(String),

    #[error("未知错误: {0}")]
    Unknown(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
