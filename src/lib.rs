//! `sunnynet-rs` 提供对 SunnyNet DLL 的 Rust 封装。
//!
//! 设计上分为两层：
//! - `LoadedSunnyNet`：底层动态加载 + 原生符号调用，适合精细控制。
//! - `SunnyNetClient`：高层客户端封装，适合业务直接接入事件回调。

mod callbacks;
mod cert;
mod client;
mod dynamic_loader;
mod error;
mod events;
mod handler;
mod internal;
mod native_symbols;

use libloading::Library;
use std::{
    ffi::{c_char, CStr, CString},
    path::Path,
    sync::Arc,
};

pub use cert::{CertInstallResult, CertManager, CertOptions};
pub use client::{
    DriverMode, ProcessManager, SunnyNetBuilder, SunnyNetClient, SunnyNetSource, SunnyNetStatus,
};
pub use error::{SunnyNetError, SunnyNetResult};
pub use events::{
    HttpEvent, HttpEventType, HttpRequestRef, HttpResponseRef, ScriptLogEvent, ScriptSaveEvent,
    TcpConnectionRef, TcpEvent, TcpEventType, TrafficTarget, UdpEvent, UdpEventType, UdpPacketRef,
    WebSocketEvent, WebSocketEventType, WebSocketMessageType, WebSocketRef,
};
pub use handler::{
    HttpHandler, NoopHandler, ScriptHandler, SunnyNetHandler, TcpHandler, UdpHandler,
    WebSocketHandler,
};
pub use native_symbols::SUNNYNET_NATIVE_SYMBOLS;

// 来自 SunnyNet64.h：64 位下 GoInt 对应 GoInt64
/// SunnyNet 在 64 位环境中使用的 Go 整型。
pub type GoInt = i64;
/// SunnyNet 在 64 位环境中使用的 Go 64 位整型。
pub type GoInt64 = i64;
/// Go 侧 `uint8` 映射类型。
pub type GoUint8 = u8;
/// Go 指针宽度整型，用于承载 DLL 返回地址。
pub type GoUintptr = usize;
/// Go 侧 `float64` 映射类型。
pub type GoFloat64 = f64;

type CreateSunnyNetFn = unsafe extern "C" fn() -> GoInt;
type ReleaseSunnyNetFn = unsafe extern "C" fn(GoInt) -> GoUint8;
type SunnyNetSetPortFn = unsafe extern "C" fn(GoInt, GoInt) -> GoUint8;
type SunnyNetSetCallbackFn = unsafe extern "C" fn(GoInt, GoInt, GoInt, GoInt, GoInt) -> GoUint8;
type SunnyNetStartFn = unsafe extern "C" fn(GoInt) -> GoUint8;
type SunnyNetCloseFn = unsafe extern "C" fn(GoInt) -> GoUint8;
type SunnyNetErrorFn = unsafe extern "C" fn(GoInt) -> GoUintptr;
type CreateCertificateFn = unsafe extern "C" fn() -> GoInt;
type RemoveCertificateFn = unsafe extern "C" fn(GoInt);
type CreateCAFn = unsafe extern "C" fn(
    GoInt,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    GoInt,
    GoInt,
) -> GoUint8;
type ExportCAFn = unsafe extern "C" fn(GoInt) -> GoUintptr;
type ExportKeyFn = unsafe extern "C" fn(GoInt) -> GoUintptr;
type LoadX509KeyPairFn = unsafe extern "C" fn(GoInt, *const c_char, *const c_char) -> GoUint8;
type SunnyNetSetCertFn = unsafe extern "C" fn(GoInt, GoInt) -> GoUint8;
type SunnyNetInstallCertFn = unsafe extern "C" fn(GoInt) -> GoUintptr;
type SetIeProxyFn = unsafe extern "C" fn(GoInt) -> GoUint8;
type CancelIeProxyFn = unsafe extern "C" fn(GoInt) -> GoUint8;
type FreeFn = unsafe extern "C" fn(GoUintptr);
type GetSunnyVersionFn = unsafe extern "C" fn() -> GoUintptr;

/// SunnyNet 原生函数表。
///
/// 该类型持有从 DLL 中解析出的函数指针，适合需要逐个调用底层 API 的场景。
#[derive(Clone)]
pub struct SunnyNetApi {
    create_sunny_net: CreateSunnyNetFn,
    release_sunny_net: ReleaseSunnyNetFn,
    sunny_net_set_port: SunnyNetSetPortFn,
    sunny_net_set_callback: SunnyNetSetCallbackFn,
    sunny_net_start: SunnyNetStartFn,
    sunny_net_close: SunnyNetCloseFn,
    sunny_net_error: SunnyNetErrorFn,
    create_certificate: CreateCertificateFn,
    remove_certificate: RemoveCertificateFn,
    create_ca: CreateCAFn,
    export_ca: ExportCAFn,
    export_key: ExportKeyFn,
    load_x509_key_pair: LoadX509KeyPairFn,
    sunny_net_set_cert: SunnyNetSetCertFn,
    sunny_net_install_cert: Option<SunnyNetInstallCertFn>,
    set_ie_proxy: SetIeProxyFn,
    cancel_ie_proxy: CancelIeProxyFn,
    free: FreeFn,
    get_sunny_version: GetSunnyVersionFn,
}

macro_rules! req_symbol {
    ($library:expr, $ty:ty, $symbol:literal) => {
        dynamic_loader::load_required::<$ty>($library, $symbol)?
    };
}

macro_rules! opt_symbol {
    ($library:expr, $ty:ty, $symbol:literal) => {
        dynamic_loader::load_optional::<$ty>($library, $symbol)
    };
}

macro_rules! build_sunnynet_api {
    ($library:expr) => {
        SunnyNetApi {
            create_sunny_net: req_symbol!($library, CreateSunnyNetFn, "CreateSunnyNet"),
            release_sunny_net: req_symbol!($library, ReleaseSunnyNetFn, "ReleaseSunnyNet"),
            sunny_net_set_port: req_symbol!($library, SunnyNetSetPortFn, "SunnyNetSetPort"),
            sunny_net_set_callback: req_symbol!(
                $library,
                SunnyNetSetCallbackFn,
                "SunnyNetSetCallback"
            ),
            sunny_net_start: req_symbol!($library, SunnyNetStartFn, "SunnyNetStart"),
            sunny_net_close: req_symbol!($library, SunnyNetCloseFn, "SunnyNetClose"),
            sunny_net_error: req_symbol!($library, SunnyNetErrorFn, "SunnyNetError"),
            create_certificate: req_symbol!($library, CreateCertificateFn, "CreateCertificate"),
            remove_certificate: req_symbol!($library, RemoveCertificateFn, "RemoveCertificate"),
            create_ca: req_symbol!($library, CreateCAFn, "CreateCA"),
            export_ca: req_symbol!($library, ExportCAFn, "ExportCA"),
            export_key: req_symbol!($library, ExportKeyFn, "ExportKEY"),
            load_x509_key_pair: req_symbol!($library, LoadX509KeyPairFn, "LoadX509KeyPair"),
            sunny_net_set_cert: req_symbol!($library, SunnyNetSetCertFn, "SunnyNetSetCert"),
            sunny_net_install_cert: opt_symbol!(
                $library,
                SunnyNetInstallCertFn,
                "SunnyNetInstallCert"
            ),
            set_ie_proxy: req_symbol!($library, SetIeProxyFn, "SetIeProxy"),
            cancel_ie_proxy: req_symbol!($library, CancelIeProxyFn, "CancelIEProxy"),
            free: req_symbol!($library, FreeFn, "Free"),
            get_sunny_version: req_symbol!($library, GetSunnyVersionFn, "GetSunnyVersion"),
        }
    };
}

impl SunnyNetApi {
    /// 创建 SunnyNet 上下文，返回值小于等于 0 表示失败。
    pub fn create_sunny_net(&self) -> GoInt {
        unsafe { (self.create_sunny_net)() }
    }

    /// 释放 SunnyNet 上下文。
    pub fn release_sunny_net(&self, sunny_context: GoInt) -> bool {
        unsafe { (self.release_sunny_net)(sunny_context) != 0 }
    }

    /// 设置监听端口。
    pub fn sunny_net_set_port(&self, sunny_context: GoInt, port: GoInt) -> bool {
        unsafe { (self.sunny_net_set_port)(sunny_context, port) != 0 }
    }

    /// 注册 HTTP/TCP/WebSocket/UDP 回调地址。
    pub fn sunny_net_set_callback(
        &self,
        sunny_context: GoInt,
        callback: GoInt,
        tcp_callback: GoInt,
        ws_callback: GoInt,
        udp_callback: GoInt,
    ) -> bool {
        unsafe {
            (self.sunny_net_set_callback)(
                sunny_context,
                callback,
                tcp_callback,
                ws_callback,
                udp_callback,
            ) != 0
        }
    }

    pub fn sunny_net_start(&self, sunny_context: GoInt) -> bool {
        unsafe { (self.sunny_net_start)(sunny_context) != 0 }
    }

    /// 停止并关闭代理服务。
    pub fn sunny_net_close(&self, sunny_context: GoInt) -> bool {
        unsafe { (self.sunny_net_close)(sunny_context) != 0 }
    }

    /// 获取错误信息指针（需配合 `read_owned_string` 读取并回收）。
    pub fn sunny_net_error_ptr(&self, sunny_context: GoInt) -> GoUintptr {
        unsafe { (self.sunny_net_error)(sunny_context) }
    }

    /// 创建证书上下文。
    pub fn create_certificate(&self) -> GoInt {
        unsafe { (self.create_certificate)() }
    }

    /// 释放证书上下文。
    pub fn remove_certificate(&self, certificate_context: GoInt) {
        unsafe { (self.remove_certificate)(certificate_context) }
    }

    /// 原始参数版本的创建 CA 接口。
    ///
    /// # Safety
    /// 调用方需确保传入的 C 字符串指针有效，且生命周期覆盖本次调用。
    pub fn create_ca_raw(
        &self,
        certificate_context: GoInt,
        country: *const c_char,
        organization: *const c_char,
        organizational_unit: *const c_char,
        province: *const c_char,
        common_name: *const c_char,
        locality: *const c_char,
        bits: GoInt,
        not_after_days: GoInt,
    ) -> bool {
        unsafe {
            (self.create_ca)(
                certificate_context,
                country,
                organization,
                organizational_unit,
                province,
                common_name,
                locality,
                bits,
                not_after_days,
            ) != 0
        }
    }

    pub fn create_ca(
        &self,
        certificate_context: GoInt,
        country: &str,
        organization: &str,
        organizational_unit: &str,
        province: &str,
        common_name: &str,
        locality: &str,
        bits: GoInt,
        not_after_days: GoInt,
    ) -> Result<bool, String> {
        let country =
            CString::new(country).map_err(|err| format!("cstring_country_failed: {err}"))?;
        let organization = CString::new(organization)
            .map_err(|err| format!("cstring_organization_failed: {err}"))?;
        let organizational_unit = CString::new(organizational_unit)
            .map_err(|err| format!("cstring_organizational_unit_failed: {err}"))?;
        let province =
            CString::new(province).map_err(|err| format!("cstring_province_failed: {err}"))?;
        let common_name = CString::new(common_name)
            .map_err(|err| format!("cstring_common_name_failed: {err}"))?;
        let locality =
            CString::new(locality).map_err(|err| format!("cstring_locality_failed: {err}"))?;

        Ok(self.create_ca_raw(
            certificate_context,
            country.as_ptr(),
            organization.as_ptr(),
            organizational_unit.as_ptr(),
            province.as_ptr(),
            common_name.as_ptr(),
            locality.as_ptr(),
            bits,
            not_after_days,
        ))
    }

    pub fn export_ca_ptr(&self, certificate_context: GoInt) -> GoUintptr {
        unsafe { (self.export_ca)(certificate_context) }
    }

    /// 导出私钥字符串指针。
    pub fn export_key_ptr(&self, certificate_context: GoInt) -> GoUintptr {
        unsafe { (self.export_key)(certificate_context) }
    }

    pub fn export_ca_pem(&self, certificate_context: GoInt) -> Option<String> {
        self.read_owned_string(self.export_ca_ptr(certificate_context))
    }

    pub fn export_key_pem(&self, certificate_context: GoInt) -> Option<String> {
        self.read_owned_string(self.export_key_ptr(certificate_context))
    }

    /// 原始参数版本的加载证书/私钥对接口。
    ///
    /// # Safety
    /// 调用方需确保路径指针为有效 C 字符串。
    pub fn load_x509_key_pair_raw(
        &self,
        certificate_context: GoInt,
        cert_path: *const c_char,
        key_path: *const c_char,
    ) -> bool {
        unsafe { (self.load_x509_key_pair)(certificate_context, cert_path, key_path) != 0 }
    }

    pub fn load_x509_key_pair(
        &self,
        certificate_context: GoInt,
        cert_path: &Path,
        key_path: &Path,
    ) -> Result<bool, String> {
        let cert_path = CString::new(cert_path.to_string_lossy().to_string())
            .map_err(|err| format!("cstring_cert_path_failed: {err}"))?;
        let key_path = CString::new(key_path.to_string_lossy().to_string())
            .map_err(|err| format!("cstring_key_path_failed: {err}"))?;
        Ok(self.load_x509_key_pair_raw(certificate_context, cert_path.as_ptr(), key_path.as_ptr()))
    }

    pub fn sunny_net_set_cert(&self, sunny_context: GoInt, certificate_context: GoInt) -> bool {
        unsafe { (self.sunny_net_set_cert)(sunny_context, certificate_context) != 0 }
    }

    /// 执行系统证书安装（若 DLL 支持该导出）。
    pub fn sunny_net_install_cert_ptr(&self, sunny_context: GoInt) -> Option<GoUintptr> {
        self.sunny_net_install_cert
            .map(|func| unsafe { func(sunny_context) })
    }

    pub fn set_ie_proxy(&self, sunny_context: GoInt) -> bool {
        unsafe { (self.set_ie_proxy)(sunny_context) != 0 }
    }

    pub fn cancel_ie_proxy(&self, sunny_context: GoInt) -> bool {
        unsafe { (self.cancel_ie_proxy)(sunny_context) != 0 }
    }

    pub fn get_sunny_version_ptr(&self) -> GoUintptr {
        unsafe { (self.get_sunny_version)() }
    }

    /// 释放由 DLL 分配的内存。
    pub fn free(&self, ptr: GoUintptr) {
        unsafe { (self.free)(ptr) }
    }

    /// 读取并回收 DLL 返回的字符串。
    ///
    /// 当 `ptr == 0` 或字符串为空时返回 `None`。
    pub fn read_owned_string(&self, ptr: GoUintptr) -> Option<String> {
        if ptr == 0 {
            return None;
        }

        let text = unsafe {
            let text = CStr::from_ptr(ptr as *const c_char)
                .to_string_lossy()
                .trim()
                .to_string();
            (self.free)(ptr);
            text
        };
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

/// 已加载的 SunnyNet DLL 句柄与 API 入口。
pub struct LoadedSunnyNet {
    library: Arc<Library>,
    api: Arc<SunnyNetApi>,
}

impl LoadedSunnyNet {
    /// 从给定路径加载 SunnyNet DLL，并解析核心符号。
    pub fn load(path: &Path) -> Result<Self, String> {
        unsafe {
            let library = Arc::new(
                Library::new(path)
                    .map_err(|err| format!("load_dll_failed {}: {err}", path.display()))?,
            );
            let api = build_sunnynet_api!(library.as_ref());

            Ok(Self {
                library,
                api: Arc::new(api),
            })
        }
    }

    /// 获取底层 API 函数表。
    pub fn api(&self) -> Arc<SunnyNetApi> {
        self.api.clone()
    }

    /// 读取 DLL 版本字符串。
    pub fn get_version(&self) -> Option<String> {
        self.api.read_owned_string(self.api.get_sunny_version_ptr())
    }

    /// 读取并回收 DLL 返回的字符串。
    pub fn read_owned_string(&self, ptr: GoUintptr) -> Option<String> {
        self.api.read_owned_string(ptr)
    }

    // 对应头文件中的完整导出表。若尚未为某个函数提供强类型包装，可通过此接口先行接入。
    /// 通过原始符号名解析任意导出函数。
    ///
    /// # Safety
    /// 调用方需确保 `T` 与目标函数真实签名完全一致。
    pub unsafe fn resolve_symbol<T>(&self, symbol: &[u8]) -> Result<T, String>
    where
        T: Copy,
    {
        let symbol_name = String::from_utf8_lossy(symbol)
            .trim_end_matches('\0')
            .to_string();
        dynamic_loader::resolve_symbol::<T>(self.library.as_ref(), &symbol_name)
    }

    /// 判断符号是否存在（符号名可带或不带 `\0`）。
    pub fn has_symbol(&self, symbol: &[u8]) -> bool {
        let symbol_name = String::from_utf8_lossy(symbol)
            .trim_end_matches('\0')
            .to_string();
        dynamic_loader::has_symbol(self.library.as_ref(), &symbol_name)
    }

    /// 通过符号名解析函数。
    ///
    /// # Safety
    /// 调用方需保证签名匹配，避免未定义行为。
    pub unsafe fn resolve_symbol_by_name<T>(&self, symbol_name: &str) -> Result<T, String>
    where
        T: Copy,
    {
        let symbol = to_nul_terminated_symbol(symbol_name);
        self.resolve_symbol::<T>(&symbol)
    }

    pub fn has_symbol_by_name(&self, symbol_name: &str) -> bool {
        let symbol = to_nul_terminated_symbol(symbol_name);
        self.has_symbol(&symbol)
    }

    pub fn missing_symbols<'a>(&self, symbols: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        symbols
            .into_iter()
            .filter(|name| !self.has_symbol_by_name(name))
            .map(ToString::to_string)
            .collect()
    }

    /// 基于内置 `SUNNYNET_NATIVE_SYMBOLS` 列表检查缺失导出。
    pub fn missing_native_symbols(&self) -> Vec<String> {
        self.missing_symbols(SUNNYNET_NATIVE_SYMBOLS.iter().copied())
    }
}

fn to_nul_terminated_symbol(symbol_name: &str) -> Vec<u8> {
    let mut bytes = symbol_name.as_bytes().to_vec();
    if !bytes.ends_with(&[0]) {
        bytes.push(0);
    }
    bytes
}

macro_rules! symbol_method {
    ($name:ident, $symbol:literal, void, ($($arg:ident : $arg_ty:ty),*)) => {
        pub unsafe fn $name(&self, $($arg: $arg_ty),*) -> Result<(), String> {
            type FnTy = unsafe extern "C" fn($($arg_ty),*);
            let func: FnTy = self.resolve_symbol_by_name::<FnTy>($symbol)?;
            func($($arg),*);
            Ok(())
        }
    };
    ($name:ident, $symbol:literal, $ret:ty, ($($arg:ident : $arg_ty:ty),*)) => {
        pub unsafe fn $name(&self, $($arg: $arg_ty),*) -> Result<$ret, String> {
            type FnTy = unsafe extern "C" fn($($arg_ty),*) -> $ret;
            let func: FnTy = self.resolve_symbol_by_name::<FnTy>($symbol)?;
            Ok(func($($arg),*))
        }
    };
}

mod generated_symbol_methods;
