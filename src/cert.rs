use crate::{
    client::SunnyNetClient, handler::SunnyNetHandler, internal::SharedRuntime, GoInt,
    SunnyNetError, SunnyNetResult,
};
use std::path::Path;

/// 创建 CA 时的可配置参数。
#[derive(Clone, Debug)]
pub struct CertOptions {
    /// 国家。
    pub country: String,
    /// 组织名。
    pub organization: String,
    /// 组织单元。
    pub organizational_unit: String,
    /// 省/州。
    pub province: String,
    /// 通用名称（CN）。
    pub common_name: String,
    /// 城市。
    pub locality: String,
    /// 密钥位数。
    pub bits: u32,
    /// 证书有效天数。
    pub not_after_days: u32,
}

impl Default for CertOptions {
    fn default() -> Self {
        Self {
            country: "CN".to_string(),
            organization: "SunnyNet".to_string(),
            organizational_unit: "SunnyNet".to_string(),
            province: "Beijing".to_string(),
            common_name: "SunnyNet".to_string(),
            locality: "Beijing".to_string(),
            bits: 2048,
            not_after_days: 3650,
        }
    }
}

/// 证书安装结果。
#[derive(Clone, Debug)]
pub struct CertInstallResult {
    /// 是否识别为成功。
    pub success: bool,
    /// 安装器返回信息。
    pub message: String,
}

impl CertInstallResult {
    pub(crate) fn from_message(message: Option<String>) -> Self {
        let message = message
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "证书安装命令已执行，请检查系统证书列表".to_string());
        let lowered = message.to_ascii_lowercase();
        let success = lowered.contains("already in store")
            || lowered.contains("completed successfully")
            || message.contains("已在存储")
            || message.contains("添加到存储");
        Self { success, message }
    }
}

/// SunnyNet 证书管理器。
pub struct CertManager {
    runtime: SharedRuntime,
    context: GoInt,
}

impl CertManager {
    pub(crate) fn new(runtime: SharedRuntime) -> SunnyNetResult<Self> {
        let context = runtime.api().create_certificate();
        if context <= 0 {
            return Err(SunnyNetError::operation_failed(
                "CreateCertificate",
                "create_certificate_failed",
            ));
        }
        Ok(Self { runtime, context })
    }

    /// 获取证书上下文 ID。
    pub fn context(&self) -> GoInt {
        self.context
    }

    /// 创建自签 CA。
    pub fn create_ca(&self, options: &CertOptions) -> SunnyNetResult<()> {
        let created = self
            .runtime
            .api()
            .create_ca(
                self.context,
                &options.country,
                &options.organization,
                &options.organizational_unit,
                &options.province,
                &options.common_name,
                &options.locality,
                GoInt::from(options.bits),
                GoInt::from(options.not_after_days),
            )
            .map_err(|err| SunnyNetError::operation_failed("CreateCA", err))?;
        if created {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "CreateCA",
                "create_ca_failed",
            ))
        }
    }

    /// 导出 CA 证书（PEM）。
    pub fn export_ca_pem(&self) -> Option<String> {
        self.runtime.api().export_ca_pem(self.context)
    }

    /// 导出私钥（PEM）。
    pub fn export_key_pem(&self) -> Option<String> {
        self.runtime.api().export_key_pem(self.context)
    }

    /// 加载已有证书和私钥。
    pub fn load_x509_key_pair(&self, cert_path: &Path, key_path: &Path) -> SunnyNetResult<()> {
        let loaded = self
            .runtime
            .api()
            .load_x509_key_pair(self.context, cert_path, key_path)
            .map_err(|err| SunnyNetError::operation_failed("LoadX509KeyPair", err))?;
        if loaded {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "LoadX509KeyPair",
                "load_x509_key_pair_failed",
            ))
        }
    }

    /// 将证书应用到指定客户端。
    pub fn apply_to<H>(&self, client: &SunnyNetClient<H>) -> SunnyNetResult<()>
    where
        H: SunnyNetHandler,
    {
        client.set_certificate_context(self.context)
    }
}

impl Drop for CertManager {
    fn drop(&mut self) {
        self.runtime.api().remove_certificate(self.context);
    }
}

#[cfg(test)]
mod tests {
    use super::CertInstallResult;

    #[test]
    fn cert_install_result_detects_known_success_messages() {
        assert!(CertInstallResult::from_message(Some("already in store".to_string())).success);
        assert!(
            CertInstallResult::from_message(Some(
                "CertUtil: -addstore command completed successfully.".to_string()
            ))
            .success
        );
        assert!(!CertInstallResult::from_message(Some("unknown failure".to_string())).success);
    }
}
