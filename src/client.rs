use crate::{
    callbacks::{register_handler, unregister_handler, CallbackAddresses},
    cert::{CertInstallResult, CertManager},
    handler::SunnyNetHandler,
    internal::{bool_to_flag, cstring_arg, SharedRuntime},
    GoInt, GoInt64, LoadedSunnyNet, SunnyNetError, SunnyNetResult,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
const EMBEDDED_DLL: &[u8] = include_bytes!("../bin/SunnyNet64.dll");
#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
const EMBEDDED_DLL: &[u8] = include_bytes!("../bin/SunnyNet.dll");
#[cfg(not(target_os = "windows"))]
const EMBEDDED_DLL: &[u8] = &[];

/// SunnyNet DLL 的加载来源。
#[derive(Clone, Debug)]
pub enum SunnyNetSource {
    /// 使用 crate 内嵌 DLL（仅 Windows 有效）。
    Embedded,
    /// 使用外部 DLL 路径。
    Path(PathBuf),
}

impl Default for SunnyNetSource {
    fn default() -> Self {
        Self::Embedded
    }
}

/// 客户端状态快照。
#[derive(Clone, Debug)]
pub struct SunnyNetStatus {
    /// 当前监听端口。
    pub port: u16,
    /// 当前是否运行中。
    pub running: bool,
    /// 是否已开启系统代理。
    pub system_proxy_enabled: bool,
    /// DLL 实际路径。
    pub dll_path: Option<String>,
    /// DLL 版本信息。
    pub version: Option<String>,
    /// 最近一次错误。
    pub last_error: Option<String>,
}

/// 驱动加载模式。
#[derive(Clone, Copy, Debug)]
pub enum DriverMode {
    Proxifier,
    Nfapi,
    Tun,
}

impl DriverMode {
    fn as_go_int(self) -> GoInt {
        match self {
            Self::Proxifier => 0,
            Self::Nfapi => 1,
            Self::Tun => 2,
        }
    }
}

#[derive(Debug)]
struct ClientState {
    port: u16,
    running: bool,
    system_proxy_enabled: bool,
    dll_path: Option<PathBuf>,
    version: Option<String>,
    last_error: Option<String>,
}

/// `SunnyNetClient` 构建器。
pub struct SunnyNetBuilder<H> {
    handler: H,
    port: u16,
    source: SunnyNetSource,
}

/// 高层 SunnyNet 客户端封装。
///
/// 负责运行时加载、回调注册与生命周期管理。
pub struct SunnyNetClient<H> {
    runtime: SharedRuntime,
    context: GoInt,
    _handler: Arc<H>,
    state: Mutex<ClientState>,
}

/// 进程过滤管理器。
pub struct ProcessManager {
    runtime: SharedRuntime,
    context: GoInt,
}

impl<H> SunnyNetBuilder<H>
where
    H: SunnyNetHandler,
{
    /// 创建构建器，默认端口 `20250`。
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            port: 20250,
            source: SunnyNetSource::Embedded,
        }
    }

    /// 设置监听端口。
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// 设置 DLL 来源。
    pub fn source(mut self, source: SunnyNetSource) -> Self {
        self.source = source;
        self
    }

    /// 构建客户端并完成初始化。
    pub fn build(self) -> SunnyNetResult<SunnyNetClient<H>> {
        let (runtime, dll_path) = load_runtime(&self.source)?;
        let version = runtime.get_version();
        let context = create_context(&runtime)?;
        let handler = Arc::new(self.handler);

        register_handler(runtime.clone(), context, handler.clone())?;
        if let Err(err) = configure_callbacks(&runtime, context) {
            unregister_handler(context);
            cleanup_context(&runtime, context);
            return Err(err);
        }
        if let Err(err) = configure_script_callbacks(&runtime, context) {
            unregister_handler(context);
            cleanup_context(&runtime, context);
            return Err(err);
        }
        if let Err(err) = apply_listen_port(&runtime, context, self.port) {
            unregister_handler(context);
            cleanup_context(&runtime, context);
            return Err(err);
        }

        Ok(SunnyNetClient {
            runtime,
            context,
            _handler: handler,
            state: Mutex::new(ClientState {
                port: self.port,
                running: false,
                system_proxy_enabled: false,
                dll_path,
                version,
                last_error: None,
            }),
        })
    }
}

impl<H> SunnyNetClient<H>
where
    H: SunnyNetHandler,
{
    /// 创建构建器。
    pub fn builder(handler: H) -> SunnyNetBuilder<H> {
        SunnyNetBuilder::new(handler)
    }

    /// 获取 SunnyNet 上下文 ID。
    pub fn context(&self) -> GoInt {
        self.context
    }

    /// 获取状态快照。
    pub fn status(&self) -> SunnyNetStatus {
        let state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        SunnyNetStatus {
            port: state.port,
            running: state.running,
            system_proxy_enabled: state.system_proxy_enabled,
            dll_path: state
                .dll_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            version: state.version.clone(),
            last_error: state.last_error.clone(),
        }
    }

    /// 启动代理服务。
    pub fn start(&self) -> SunnyNetResult<()> {
        {
            let state = self.state.lock().unwrap_or_else(|err| err.into_inner());
            if state.running {
                return Ok(());
            }
        }

        let started = self.runtime.api().sunny_net_start(self.context);
        if !started {
            let err = self
                .runtime
                .api()
                .read_owned_string(self.runtime.api().sunny_net_error_ptr(self.context))
                .unwrap_or_else(|| "sunnynet_start_failed".to_string());
            self.set_last_error(Some(err.clone()));
            return Err(SunnyNetError::operation_failed("SunnyNetStart", err));
        }

        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        state.running = true;
        state.last_error = None;
        Ok(())
    }

    /// 停止代理服务。
    pub fn stop(&self) -> SunnyNetResult<()> {
        let proxy_enabled = {
            let state = self.state.lock().unwrap_or_else(|err| err.into_inner());
            if !state.running {
                return Ok(());
            }
            state.system_proxy_enabled
        };

        if proxy_enabled {
            let _ = self.set_system_proxy_enabled(false);
        }

        self.runtime.api().sunny_net_close(self.context);
        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        state.running = false;
        Ok(())
    }

    /// 重启代理服务。
    pub fn restart(&self) -> SunnyNetResult<()> {
        self.stop()?;
        self.start()
    }

    /// 设置监听端口（运行中不可变更）。
    pub fn set_port(&self, port: u16) -> SunnyNetResult<()> {
        {
            let state = self.state.lock().unwrap_or_else(|err| err.into_inner());
            if state.running {
                return Err(SunnyNetError::operation_failed(
                    "SunnyNetSetPort",
                    "client_running_port_change_requires_restart",
                ));
            }
        }
        apply_listen_port(&self.runtime, self.context, port)?;
        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        state.port = port;
        Ok(())
    }

    /// 启用或禁用系统代理。
    pub fn set_system_proxy_enabled(&self, enabled: bool) -> SunnyNetResult<()> {
        if enabled {
            let state = self.state.lock().unwrap_or_else(|err| err.into_inner());
            if !state.running {
                return Err(SunnyNetError::operation_failed(
                    "SetIeProxy",
                    "proxy_not_started",
                ));
            }
        }

        let success = if enabled {
            self.runtime.api().set_ie_proxy(self.context)
        } else {
            self.runtime.api().cancel_ie_proxy(self.context)
        };
        if !success {
            let op = if enabled {
                "SetIeProxy"
            } else {
                "CancelIEProxy"
            };
            let err = format!("{op}_failed");
            self.set_last_error(Some(err.clone()));
            return Err(SunnyNetError::operation_failed(op, err));
        }

        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        state.system_proxy_enabled = enabled;
        state.last_error = None;
        Ok(())
    }

    /// 调用 DLL 提供的证书安装能力。
    pub fn install_cert(&self) -> SunnyNetResult<CertInstallResult> {
        let message = match self.runtime.api().sunny_net_install_cert_ptr(self.context) {
            Some(ptr) => self.runtime.read_owned_string(ptr),
            None => {
                return Err(SunnyNetError::operation_failed(
                    "SunnyNetInstallCert",
                    "sunnynet_install_cert_not_supported",
                ));
            }
        };
        let result = CertInstallResult::from_message(message);
        self.set_last_error(None);
        Ok(result)
    }

    /// 创建证书管理器。
    pub fn create_cert_manager(&self) -> SunnyNetResult<CertManager> {
        CertManager::new(self.runtime.clone())
    }

    pub(crate) fn set_certificate_context(&self, cert_context: GoInt) -> SunnyNetResult<()> {
        let success = self
            .runtime
            .api()
            .sunny_net_set_cert(self.context, cert_context);
        if success {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "SunnyNetSetCert",
                "set_certificate_failed",
            ))
        }
    }

    /// 设置脚本页面内容。
    pub fn set_script_page(&self, page: &str) -> SunnyNetResult<String> {
        let page = cstring_arg("page", page)?;
        let ptr = unsafe { self.runtime.SetScriptPage(self.context, page.as_ptr()) }
            .map_err(|err| SunnyNetError::operation_failed("SetScriptPage", err))?;
        Ok(self.runtime.read_owned_string(ptr).unwrap_or_default())
    }

    /// 设置脚本代码内容。
    pub fn set_script_code(&self, code: &str) -> SunnyNetResult<String> {
        let bytes = code.as_bytes();
        let ptr = unsafe {
            self.runtime.SetScriptCode(
                self.context,
                bytes.as_ptr() as usize,
                GoInt::try_from(bytes.len()).unwrap_or_default(),
            )
        }
        .map_err(|err| SunnyNetError::operation_failed("SetScriptCode", err))?;
        Ok(self.runtime.read_owned_string(ptr).unwrap_or_default())
    }

    /// 判断 DLL 是否支持脚本能力。
    pub fn is_script_supported(&self) -> SunnyNetResult<bool> {
        let page = self.set_script_page("")?;
        Ok(!page.to_ascii_lowercase().contains("no"))
    }

    /// 设置是否强制使用 TCP。
    pub fn set_must_tcp(&self, enabled: bool) -> SunnyNetResult<()> {
        unsafe {
            self.runtime
                .SunnyNetMustTcp(self.context, bool_to_flag(enabled))
        }
        .map_err(|err| SunnyNetError::operation_failed("SunnyNetMustTcp", err))?;
        Ok(())
    }

    /// 设置允许修改 HTTP 请求体的最大长度。
    pub fn set_http_request_max_update_length(&self, max_len: GoInt64) -> SunnyNetResult<()> {
        let max_len = validate_http_request_max_update_length(max_len)?;

        let success = unsafe {
            self.runtime
                .SetHTTPRequestMaxUpdateLength(self.context, max_len)
        }
        .map_err(|err| SunnyNetError::operation_failed("SetHTTPRequestMaxUpdateLength", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "SetHTTPRequestMaxUpdateLength",
                "set_http_request_max_update_length_failed",
            ))
        }
    }

    /// 开关账号认证模式。
    pub fn set_auth_mode(&self, enabled: bool) -> SunnyNetResult<()> {
        let success = unsafe {
            self.runtime
                .SunnyNetVerifyUser(self.context, bool_to_flag(enabled))
        }
        .map_err(|err| SunnyNetError::operation_failed("SunnyNetVerifyUser", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "SunnyNetVerifyUser",
                "verify_user_failed",
            ))
        }
    }

    /// 添加 SOCKS5 用户。
    pub fn socket5_add_user(&self, username: &str, password: &str) -> SunnyNetResult<()> {
        let username = cstring_arg("username", username)?;
        let password = cstring_arg("password", password)?;
        let success = unsafe {
            self.runtime
                .SunnyNetSocket5AddUser(self.context, username.as_ptr(), password.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SunnyNetSocket5AddUser", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "SunnyNetSocket5AddUser",
                "add_socket5_user_failed",
            ))
        }
    }

    /// 删除 SOCKS5 用户。
    pub fn socket5_remove_user(&self, username: &str) -> SunnyNetResult<()> {
        let username = cstring_arg("username", username)?;
        let success = unsafe {
            self.runtime
                .SunnyNetSocket5DelUser(self.context, username.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SunnyNetSocket5DelUser", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "SunnyNetSocket5DelUser",
                "remove_socket5_user_failed",
            ))
        }
    }

    /// 设置全局上游代理。
    pub fn set_global_proxy(&self, proxy: &str, timeout_ms: u32) -> SunnyNetResult<()> {
        let proxy = cstring_arg("proxy", proxy)?;
        let success = unsafe {
            self.runtime
                .SetGlobalProxy(self.context, proxy.as_ptr(), GoInt::from(timeout_ms))
        }
        .map_err(|err| SunnyNetError::operation_failed("SetGlobalProxy", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "SetGlobalProxy",
                "set_global_proxy_failed",
            ))
        }
    }

    /// 清空全局上游代理。
    pub fn clear_global_proxy(&self) -> SunnyNetResult<()> {
        self.set_global_proxy("", 30_000)
    }

    /// 设置强制 TCP 规则。
    pub fn set_must_tcp_rule(&self, rule: &str, rule_type: bool) -> SunnyNetResult<()> {
        let rule = cstring_arg("rule", rule)?;
        let success = unsafe {
            self.runtime
                .SetMustTcpRegexp(self.context, rule.as_ptr(), bool_to_flag(rule_type))
        }
        .map_err(|err| SunnyNetError::operation_failed("SetMustTcpRegexp", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "SetMustTcpRegexp",
                "set_must_tcp_rule_failed",
            ))
        }
    }

    /// 设置全局代理匹配规则。
    pub fn set_global_proxy_rule(&self, rule: &str) -> SunnyNetResult<()> {
        let rule = cstring_arg("rule", rule)?;
        let success = unsafe { self.runtime.CompileProxyRegexp(self.context, rule.as_ptr()) }
            .map_err(|err| SunnyNetError::operation_failed("CompileProxyRegexp", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "CompileProxyRegexp",
                "set_global_proxy_rule_failed",
            ))
        }
    }

    /// 启用/禁用 TCP 抓取。
    pub fn disable_tcp(&self, disabled: bool) -> SunnyNetResult<()> {
        let success = unsafe {
            self.runtime
                .DisableTCP(self.context, bool_to_flag(disabled))
        }
        .map_err(|err| SunnyNetError::operation_failed("DisableTCP", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "DisableTCP",
                "disable_tcp_failed",
            ))
        }
    }

    /// 启用/禁用 UDP 抓取。
    pub fn disable_udp(&self, disabled: bool) -> SunnyNetResult<()> {
        let success = unsafe {
            self.runtime
                .DisableUDP(self.context, bool_to_flag(disabled))
        }
        .map_err(|err| SunnyNetError::operation_failed("DisableUDP", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "DisableUDP",
                "disable_udp_failed",
            ))
        }
    }

    /// 获取进程管理器。
    pub fn processes(&self) -> ProcessManager {
        ProcessManager {
            runtime: self.runtime.clone(),
            context: self.context,
        }
    }

    fn set_last_error(&self, last_error: Option<String>) {
        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        state.last_error = last_error;
    }
}

impl<H> Drop for SunnyNetClient<H> {
    fn drop(&mut self) {
        unregister_handler(self.context);
        let _ = self.runtime.api().cancel_ie_proxy(self.context);
        let _ = self.runtime.api().sunny_net_close(self.context);
        let _ = self.runtime.api().release_sunny_net(self.context);
    }
}

impl ProcessManager {
    /// 加载驱动。
    pub fn load_driver(&self, mode: DriverMode) -> SunnyNetResult<()> {
        let success = unsafe { self.runtime.OpenDrive(self.context, mode.as_go_int()) }
            .map_err(|err| SunnyNetError::operation_failed("OpenDrive", err))?;
        if success != 0 {
            Ok(())
        } else {
            Err(SunnyNetError::operation_failed(
                "OpenDrive",
                "load_driver_failed",
            ))
        }
    }

    /// 卸载驱动。
    pub fn unload_driver(&self) -> SunnyNetResult<()> {
        unsafe { self.runtime.UnDrive(self.context) }
            .map_err(|err| SunnyNetError::operation_failed("UnDrive", err))?;
        Ok(())
    }

    /// 按进程名添加匹配规则。
    pub fn add_name(&self, process_name: &str) -> SunnyNetResult<()> {
        let process_name = cstring_arg("process_name", process_name)?;
        unsafe {
            self.runtime
                .ProcessAddName(self.context, process_name.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("ProcessAddName", err))?;
        Ok(())
    }

    /// 删除进程名规则。
    pub fn remove_name(&self, process_name: &str) -> SunnyNetResult<()> {
        let process_name = cstring_arg("process_name", process_name)?;
        unsafe {
            self.runtime
                .ProcessDelName(self.context, process_name.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("ProcessDelName", err))?;
        Ok(())
    }

    /// 按 PID 添加匹配规则。
    pub fn add_pid(&self, pid: u32) -> SunnyNetResult<()> {
        unsafe { self.runtime.ProcessAddPid(self.context, GoInt::from(pid)) }
            .map_err(|err| SunnyNetError::operation_failed("ProcessAddPid", err))?;
        Ok(())
    }

    /// 删除 PID 规则。
    pub fn remove_pid(&self, pid: u32) -> SunnyNetResult<()> {
        unsafe { self.runtime.ProcessDelPid(self.context, GoInt::from(pid)) }
            .map_err(|err| SunnyNetError::operation_failed("ProcessDelPid", err))?;
        Ok(())
    }

    /// 控制是否匹配全部进程。
    pub fn all_processes(&self, enabled: bool, stop_net: bool) -> SunnyNetResult<()> {
        unsafe {
            self.runtime
                .ProcessALLName(self.context, bool_to_flag(enabled), bool_to_flag(stop_net))
        }
        .map_err(|err| SunnyNetError::operation_failed("ProcessALLName", err))?;
        Ok(())
    }

    /// 清空全部进程规则。
    pub fn clear_all(&self) -> SunnyNetResult<()> {
        unsafe { self.runtime.ProcessCancelAll(self.context) }
            .map_err(|err| SunnyNetError::operation_failed("ProcessCancelAll", err))?;
        Ok(())
    }
}

fn load_runtime(source: &SunnyNetSource) -> SunnyNetResult<(SharedRuntime, Option<PathBuf>)> {
    let (path, owned_path) = match source {
        SunnyNetSource::Embedded => {
            let path = release_embedded_dll_to_runtime()?;
            (path.clone(), Some(path))
        }
        SunnyNetSource::Path(path) => (path.clone(), Some(path.clone())),
    };

    let runtime = LoadedSunnyNet::load(&path)
        .map(Arc::new)
        .map_err(|err| SunnyNetError::operation_failed("load_runtime", err))?;
    Ok((runtime, owned_path))
}

fn create_context(runtime: &SharedRuntime) -> SunnyNetResult<GoInt> {
    let context = runtime.api().create_sunny_net();
    if context <= 0 {
        Err(SunnyNetError::operation_failed(
            "CreateSunnyNet",
            "create_sunnynet_failed",
        ))
    } else {
        Ok(context)
    }
}

fn configure_callbacks(runtime: &SharedRuntime, context: GoInt) -> SunnyNetResult<()> {
    let addresses = CallbackAddresses::new()?;
    if runtime.api().sunny_net_set_callback(
        context,
        addresses.http,
        addresses.tcp,
        addresses.websocket,
        addresses.udp,
    ) {
        Ok(())
    } else {
        Err(SunnyNetError::operation_failed(
            "SunnyNetSetCallback",
            "set_callback_failed",
        ))
    }
}

fn configure_script_callbacks(runtime: &SharedRuntime, context: GoInt) -> SunnyNetResult<()> {
    let addresses = CallbackAddresses::new()?;
    unsafe { runtime.SetScriptCall(context, addresses.script_log, addresses.script_save) }
        .map_err(|err| SunnyNetError::operation_failed("SetScriptCall", err))?;
    Ok(())
}

fn apply_listen_port(runtime: &SharedRuntime, context: GoInt, port: u16) -> SunnyNetResult<()> {
    if runtime.api().sunny_net_set_port(context, GoInt::from(port)) {
        Ok(())
    } else {
        Err(SunnyNetError::operation_failed(
            "SunnyNetSetPort",
            "set_listen_port_failed",
        ))
    }
}

fn cleanup_context(runtime: &SharedRuntime, context: GoInt) {
    let _ = runtime.api().sunny_net_close(context);
    let _ = runtime.api().release_sunny_net(context);
}

fn validate_http_request_max_update_length(max_len: GoInt64) -> SunnyNetResult<GoInt64> {
    if max_len <= 0 {
        return Err(SunnyNetError::invalid_argument(
            "max_len",
            "must_be_positive",
        ));
    }
    Ok(max_len)
}

fn release_embedded_dll_to_runtime() -> SunnyNetResult<PathBuf> {
    if EMBEDDED_DLL.is_empty() {
        return Err(SunnyNetError::unsupported("embedded_sunnynet_dll"));
    }

    let file_name = if cfg!(target_pointer_width = "64") {
        "SunnyNet64-rs.dll"
    } else {
        "SunnyNet-rs.dll"
    };
    let runtime_dir = std::env::current_exe()
        .map_err(|err| SunnyNetError::operation_failed("current_exe", err.to_string()))?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            SunnyNetError::operation_failed("current_exe", "resolve_current_exe_dir_failed")
        })?;
    fs::create_dir_all(&runtime_dir)
        .map_err(|err| SunnyNetError::operation_failed("create_runtime_dir", err.to_string()))?;
    let dll_path = runtime_dir.join(file_name);

    let should_write = match fs::read(&dll_path) {
        Ok(existing) => existing != EMBEDDED_DLL,
        Err(_) => true,
    };
    if should_write {
        fs::write(&dll_path, EMBEDDED_DLL)
            .map_err(|err| SunnyNetError::operation_failed("write_runtime_dll", err.to_string()))?;
    }
    Ok(dll_path)
}

#[cfg(test)]
mod tests {
    use super::validate_http_request_max_update_length;

    #[test]
    fn reject_non_positive_http_request_max_update_length() {
        let result = validate_http_request_max_update_length(0);
        assert!(result.is_err());
    }

    #[test]
    fn accept_positive_http_request_max_update_length() {
        let result = validate_http_request_max_update_length(64 * 1024 * 1024);
        assert_eq!(result.unwrap(), 64 * 1024 * 1024);
    }
}
