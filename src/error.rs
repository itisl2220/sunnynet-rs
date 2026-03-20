use std::{error::Error, fmt};

/// SDK 统一结果类型。
pub type SunnyNetResult<T> = Result<T, SunnyNetError>;

/// SDK 统一错误定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SunnyNetError {
    /// 参数非法。
    InvalidArgument {
        field: &'static str,
        message: String,
    },
    /// 操作失败（通常是 DLL 调用失败）。
    OperationFailed {
        operation: &'static str,
        message: String,
    },
    /// 当前 DLL 或平台不支持此能力。
    Unsupported { feature: &'static str },
    /// 原生层返回的错误文本。
    Native(String),
}

impl SunnyNetError {
    /// 构造参数错误。
    pub fn invalid_argument(field: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidArgument {
            field,
            message: message.into(),
        }
    }

    /// 构造操作失败错误。
    pub fn operation_failed(operation: &'static str, message: impl Into<String>) -> Self {
        Self::OperationFailed {
            operation,
            message: message.into(),
        }
    }

    /// 构造不支持错误。
    pub fn unsupported(feature: &'static str) -> Self {
        Self::Unsupported { feature }
    }
}

impl fmt::Display for SunnyNetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument { field, message } => {
                write!(f, "invalid_argument[{field}]: {message}")
            }
            Self::OperationFailed { operation, message } => {
                write!(f, "operation_failed[{operation}]: {message}")
            }
            Self::Unsupported { feature } => write!(f, "unsupported_feature: {feature}"),
            Self::Native(message) => write!(f, "native_error: {message}"),
        }
    }
}

impl Error for SunnyNetError {}

impl From<String> for SunnyNetError {
    fn from(value: String) -> Self {
        Self::Native(value)
    }
}

impl From<&str> for SunnyNetError {
    fn from(value: &str) -> Self {
        Self::Native(value.to_string())
    }
}
