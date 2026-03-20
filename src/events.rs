use crate::{
    internal::{
        cstring_arg, go_int_len, join_header_values, read_borrowed_bytes, read_callback_string,
        read_len_prefixed_bytes, read_owned_bytes, read_owned_string, split_header_values,
        string_first_value, success_or, SharedRuntime,
    },
    GoInt, SunnyNetError, SunnyNetResult,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// 流量方向。
pub enum TrafficTarget {
    Client,
    Server,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// HTTP 回调事件类型。
pub enum HttpEventType {
    Request,
    Response,
    Error,
    Unknown(i32),
}

impl HttpEventType {
    pub fn from_raw(value: GoInt) -> Self {
        match value as i32 {
            1 => Self::Request,
            2 => Self::Response,
            3 => Self::Error,
            other => Self::Unknown(other),
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::Request => 1,
            Self::Response => 2,
            Self::Error => 3,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// TCP 回调事件类型。
pub enum TcpEventType {
    About,
    Connected,
    Send,
    Receive,
    Close,
    Unknown(i32),
}

impl TcpEventType {
    pub fn from_raw(value: GoInt) -> Self {
        match value as i32 {
            4 => Self::About,
            0 => Self::Connected,
            1 => Self::Send,
            2 => Self::Receive,
            3 => Self::Close,
            other => Self::Unknown(other),
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::About => 4,
            Self::Connected => 0,
            Self::Send => 1,
            Self::Receive => 2,
            Self::Close => 3,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// WebSocket 回调事件类型。
pub enum WebSocketEventType {
    Connected,
    Send,
    Receive,
    Close,
    Unknown(i32),
}

impl WebSocketEventType {
    pub fn from_raw(value: GoInt) -> Self {
        match value as i32 {
            1 => Self::Connected,
            2 => Self::Send,
            3 => Self::Receive,
            4 => Self::Close,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// WebSocket 消息类型。
pub enum WebSocketMessageType {
    Text,
    Binary,
    Close,
    Ping,
    Pong,
    Invalid,
    Unknown(i32),
}

impl WebSocketMessageType {
    pub fn from_raw(value: GoInt) -> Self {
        match value as i32 {
            1 => Self::Text,
            2 => Self::Binary,
            8 => Self::Close,
            9 => Self::Ping,
            10 => Self::Pong,
            -1 => Self::Invalid,
            other => Self::Unknown(other),
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::Text => 1,
            Self::Binary => 2,
            Self::Close => 8,
            Self::Ping => 9,
            Self::Pong => 10,
            Self::Invalid => -1,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// UDP 回调事件类型。
pub enum UdpEventType {
    Close,
    Send,
    Receive,
    Unknown(i32),
}

impl UdpEventType {
    pub fn from_raw(value: GoInt) -> Self {
        match value as i32 {
            1 => Self::Close,
            2 => Self::Send,
            3 => Self::Receive,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Clone)]
/// HTTP 请求读写视图。
pub struct HttpRequestRef {
    runtime: SharedRuntime,
    message_id: GoInt,
}

impl HttpRequestRef {
    pub(crate) fn new(runtime: SharedRuntime, message_id: GoInt) -> Self {
        Self {
            runtime,
            message_id,
        }
    }

    pub fn message_id(&self) -> GoInt {
        self.message_id
    }

    pub fn body_len(&self) -> SunnyNetResult<usize> {
        let len = unsafe { self.runtime.GetRequestBodyLen(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetRequestBodyLen", err))?;
        if len <= 0 {
            Ok(0)
        } else {
            usize::try_from(len)
                .map_err(|_| SunnyNetError::operation_failed("GetRequestBodyLen", "invalid_length"))
        }
    }

    pub fn body(&self) -> SunnyNetResult<Vec<u8>> {
        let len = self.body_len()?;
        if len == 0 {
            return Ok(Vec::new());
        }
        let ptr = unsafe { self.runtime.GetRequestBody(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetRequestBody", err))?;
        read_owned_bytes(&self.runtime, ptr, len as GoInt)
    }

    pub fn body_text(&self) -> SunnyNetResult<Option<String>> {
        let body = self.body()?;
        if body.is_empty() {
            Ok(None)
        } else {
            Ok(Some(String::from_utf8_lossy(&body).to_string()))
        }
    }

    pub fn headers(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.GetRequestAllHeader(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetRequestAllHeader", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn header_values(&self, key: &str) -> SunnyNetResult<Vec<String>> {
        let key = cstring_arg("header_key", key)?;
        let ptr = unsafe { self.runtime.GetRequestHeader(self.message_id, key.as_ptr()) }
            .map_err(|err| SunnyNetError::operation_failed("GetRequestHeader", err))?;
        Ok(read_owned_string(&self.runtime, ptr)
            .map(|text| split_header_values(&text))
            .unwrap_or_default())
    }

    pub fn header(&self, key: &str) -> SunnyNetResult<Option<String>> {
        Ok(self.header_values(key)?.into_iter().next())
    }

    pub fn cookies(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.GetRequestALLCookie(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetRequestALLCookie", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn cookie(&self, key: &str) -> SunnyNetResult<Option<String>> {
        let key = cstring_arg("cookie_key", key)?;
        let ptr = unsafe { self.runtime.GetRequestCookie(self.message_id, key.as_ptr()) }
            .map_err(|err| SunnyNetError::operation_failed("GetRequestCookie", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn proto(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.GetRequestProto(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetRequestProto", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn is_raw_body(&self) -> SunnyNetResult<bool> {
        let value = unsafe { self.runtime.IsRequestRawBody(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("IsRequestRawBody", err))?;
        Ok(value != 0)
    }

    pub fn set_proxy(&self, proxy: &str, timeout_ms: u32) -> SunnyNetResult<()> {
        let proxy = cstring_arg("proxy", proxy)?;
        let success = unsafe {
            self.runtime
                .SetRequestProxy(self.message_id, proxy.as_ptr(), GoInt::from(timeout_ms))
        }
        .map_err(|err| SunnyNetError::operation_failed("SetRequestProxy", err))?;
        success_or("SetRequestProxy", success != 0)
    }

    pub fn set_out_router_ip(&self, ip: &str) -> SunnyNetResult<()> {
        let ip = cstring_arg("out_router_ip", ip)?;
        let success = unsafe {
            self.runtime
                .RequestSetOutRouterIP(self.message_id, ip.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("RequestSetOutRouterIP", err))?;
        success_or("RequestSetOutRouterIP", success != 0)
    }

    pub fn set_timeout(&self, timeout_ms: u32) -> SunnyNetResult<()> {
        unsafe {
            self.runtime
                .SetRequestOutTime(self.message_id, GoInt::from(timeout_ms))
        }
        .map_err(|err| SunnyNetError::operation_failed("SetRequestOutTime", err))?;
        Ok(())
    }

    pub fn set_url(&self, url: &str) -> SunnyNetResult<()> {
        let url = cstring_arg("url", url)?;
        let success = unsafe { self.runtime.SetRequestUrl(self.message_id, url.as_ptr()) }
            .map_err(|err| SunnyNetError::operation_failed("SetRequestUrl", err))?;
        success_or("SetRequestUrl", success != 0)
    }

    pub fn set_body(&self, body: &[u8]) -> SunnyNetResult<()> {
        let success = unsafe {
            self.runtime.SetRequestData(
                self.message_id,
                body.as_ptr() as usize,
                go_int_len(body.len(), "SetRequestData")?,
            )
        }
        .map_err(|err| SunnyNetError::operation_failed("SetRequestData", err))?;
        success_or("SetRequestData", success != 0)
    }

    pub fn set_body_text(&self, body: &str) -> SunnyNetResult<()> {
        self.set_body(body.as_bytes())
    }

    pub fn set_all_headers(&self, headers: &str) -> SunnyNetResult<()> {
        let headers = cstring_arg("headers", headers)?;
        unsafe {
            self.runtime
                .SetRequestALLHeader(self.message_id, headers.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SetRequestALLHeader", err))?;
        Ok(())
    }

    pub fn set_header(&self, key: &str, value: &str) -> SunnyNetResult<()> {
        let key = cstring_arg("header_key", key)?;
        let value = cstring_arg("header_value", value)?;
        unsafe {
            self.runtime
                .SetRequestHeader(self.message_id, key.as_ptr(), value.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SetRequestHeader", err))?;
        Ok(())
    }

    pub fn set_header_values(&self, key: &str, values: &[String]) -> SunnyNetResult<()> {
        self.set_header(key, &join_header_values(values))
    }

    pub fn delete_header(&self, key: &str) -> SunnyNetResult<()> {
        let key = cstring_arg("header_key", key)?;
        unsafe { self.runtime.DelRequestHeader(self.message_id, key.as_ptr()) }
            .map_err(|err| SunnyNetError::operation_failed("DelRequestHeader", err))?;
        Ok(())
    }

    pub fn delete_all_headers(&self) -> SunnyNetResult<()> {
        if let Some(headers) = self.headers()? {
            for line in headers.lines() {
                if let Some((name, _)) = line.split_once(':') {
                    let _ = self.delete_header(name.trim());
                }
            }
        }
        Ok(())
    }

    pub fn remove_compression_header(&self) -> SunnyNetResult<()> {
        self.delete_header("Accept-Encoding")
    }

    pub fn stop(&self) -> SunnyNetResult<()> {
        let key = cstring_arg("header_key", "Connection")?;
        let value = cstring_arg("header_value", "Close")?;
        unsafe {
            self.runtime
                .SetResponseHeader(self.message_id, key.as_ptr(), value.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SetResponseHeader", err))?;
        Ok(())
    }

    pub fn raw_request_data_to_file(&self, path: &str) -> SunnyNetResult<()> {
        let path = cstring_arg("path", path)?;
        let success = unsafe {
            self.runtime.RawRequestDataToFile(
                self.message_id,
                path.as_ptr() as usize,
                go_int_len(path.as_bytes().len(), "RawRequestDataToFile")?,
            )
        }
        .map_err(|err| SunnyNetError::operation_failed("RawRequestDataToFile", err))?;
        success_or("RawRequestDataToFile", success != 0)
    }

    pub fn set_all_cookies(&self, cookies: &str) -> SunnyNetResult<()> {
        let cookies = cstring_arg("cookies", cookies)?;
        unsafe {
            self.runtime
                .SetRequestAllCookie(self.message_id, cookies.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SetRequestAllCookie", err))?;
        Ok(())
    }

    pub fn set_cookie(&self, key: &str, value: &str) -> SunnyNetResult<()> {
        let key = cstring_arg("cookie_key", key)?;
        let value = cstring_arg("cookie_value", value)?;
        unsafe {
            self.runtime
                .SetRequestCookie(self.message_id, key.as_ptr(), value.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SetRequestCookie", err))?;
        Ok(())
    }

    pub fn random_ja3(&self) -> SunnyNetResult<()> {
        let success = unsafe { self.runtime.RandomRequestCipherSuites(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("RandomRequestCipherSuites", err))?;
        success_or("RandomRequestCipherSuites", success != 0)
    }

    pub fn set_http2_config(&self, config: &str) -> SunnyNetResult<()> {
        let config = cstring_arg("http2_config", config)?;
        let success = unsafe {
            self.runtime
                .SetRequestHTTP2Config(self.message_id, config.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SetRequestHTTP2Config", err))?;
        success_or("SetRequestHTTP2Config", success != 0)
    }
}

#[derive(Clone)]
pub struct HttpResponseRef {
    runtime: SharedRuntime,
    message_id: GoInt,
}

impl HttpResponseRef {
    pub(crate) fn new(runtime: SharedRuntime, message_id: GoInt) -> Self {
        Self {
            runtime,
            message_id,
        }
    }

    pub fn body_len(&self) -> SunnyNetResult<usize> {
        let len = unsafe { self.runtime.GetResponseBodyLen(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetResponseBodyLen", err))?;
        if len <= 0 {
            Ok(0)
        } else {
            usize::try_from(len).map_err(|_| {
                SunnyNetError::operation_failed("GetResponseBodyLen", "invalid_length")
            })
        }
    }

    pub fn body(&self) -> SunnyNetResult<Vec<u8>> {
        let len = self.body_len()?;
        if len == 0 {
            return Ok(Vec::new());
        }
        let ptr = unsafe { self.runtime.GetResponseBody(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetResponseBody", err))?;
        read_owned_bytes(&self.runtime, ptr, len as GoInt)
    }

    pub fn text(&self) -> SunnyNetResult<Option<String>> {
        let body = self.body()?;
        if body.is_empty() {
            Ok(None)
        } else {
            Ok(Some(String::from_utf8_lossy(&body).to_string()))
        }
    }

    pub fn headers(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.GetResponseAllHeader(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetResponseAllHeader", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn header_values(&self, key: &str) -> SunnyNetResult<Vec<String>> {
        let key = cstring_arg("header_key", key)?;
        let ptr = unsafe {
            self.runtime
                .GetResponseHeader(self.message_id, key.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("GetResponseHeader", err))?;
        Ok(read_owned_string(&self.runtime, ptr)
            .map(|text| split_header_values(&text))
            .unwrap_or_default())
    }

    pub fn header(&self, key: &str) -> SunnyNetResult<Option<String>> {
        Ok(self.header_values(key)?.into_iter().next())
    }

    pub fn status_code(&self) -> SunnyNetResult<Option<i32>> {
        let code = unsafe { self.runtime.GetResponseStatusCode(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetResponseStatusCode", err))?;
        if code <= 0 {
            Ok(None)
        } else {
            Ok(i32::try_from(code).ok())
        }
    }

    pub fn status_text(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.GetResponseStatus(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetResponseStatus", err))?;
        Ok(read_owned_string(&self.runtime, ptr).and_then(string_first_value))
    }

    pub fn proto(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.GetResponseProto(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetResponseProto", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn server_address(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.GetResponseServerAddress(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetResponseServerAddress", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn set_body(&self, body: &[u8]) -> SunnyNetResult<()> {
        let success = unsafe {
            self.runtime.SetResponseData(
                self.message_id,
                body.as_ptr() as usize,
                go_int_len(body.len(), "SetResponseData")?,
            )
        }
        .map_err(|err| SunnyNetError::operation_failed("SetResponseData", err))?;
        success_or("SetResponseData", success != 0)
    }

    pub fn set_body_text(&self, body: &str) -> SunnyNetResult<()> {
        self.set_body(body.as_bytes())
    }

    pub fn set_header(&self, key: &str, value: &str) -> SunnyNetResult<()> {
        let key = cstring_arg("header_key", key)?;
        let value = cstring_arg("header_value", value)?;
        unsafe {
            self.runtime
                .SetResponseHeader(self.message_id, key.as_ptr(), value.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SetResponseHeader", err))?;
        Ok(())
    }

    pub fn set_header_values(&self, key: &str, values: &[String]) -> SunnyNetResult<()> {
        self.set_header(key, &join_header_values(values))
    }

    pub fn set_all_headers(&self, headers: &str) -> SunnyNetResult<()> {
        let headers = cstring_arg("headers", headers)?;
        unsafe {
            self.runtime
                .SetResponseAllHeader(self.message_id, headers.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SetResponseAllHeader", err))?;
        Ok(())
    }

    pub fn delete_header(&self, key: &str) -> SunnyNetResult<()> {
        let key = cstring_arg("header_key", key)?;
        unsafe {
            self.runtime
                .DelResponseHeader(self.message_id, key.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("DelResponseHeader", err))?;
        Ok(())
    }

    pub fn delete_all_headers(&self) -> SunnyNetResult<()> {
        if let Some(headers) = self.headers()? {
            for line in headers.lines() {
                if let Some((name, _)) = line.split_once(':') {
                    let _ = self.delete_header(name.trim());
                }
            }
        }
        Ok(())
    }

    pub fn set_status_code(&self, status_code: i32) -> SunnyNetResult<()> {
        let status_code = if status_code <= 0 { 200 } else { status_code };
        unsafe {
            self.runtime
                .SetResponseStatus(self.message_id, GoInt::from(status_code))
        }
        .map_err(|err| SunnyNetError::operation_failed("SetResponseStatus", err))?;
        Ok(())
    }
}
#[derive(Clone)]
pub struct HttpEvent {
    runtime: SharedRuntime,
    context: GoInt,
    request_id: GoInt,
    message_id: GoInt,
    event_type: HttpEventType,
    method: Option<String>,
    url: Option<String>,
    error: Option<String>,
    pid: Option<u32>,
}

impl HttpEvent {
    pub(crate) fn from_raw(
        runtime: SharedRuntime,
        context: GoInt,
        request_id: GoInt,
        message_id: GoInt,
        event_type: GoInt,
        method_ptr: *const std::ffi::c_char,
        url_ptr: *const std::ffi::c_char,
        error_ptr: *const std::ffi::c_char,
        pid: GoInt,
    ) -> Self {
        Self {
            runtime,
            context,
            request_id,
            message_id,
            event_type: HttpEventType::from_raw(event_type),
            method: read_callback_string(method_ptr),
            url: read_callback_string(url_ptr),
            error: read_callback_string(error_ptr),
            pid: u32::try_from(pid).ok().filter(|value| *value > 0),
        }
    }

    pub fn context(&self) -> GoInt {
        self.context
    }

    pub fn unique_id(&self) -> GoInt {
        self.request_id
    }

    pub fn message_id(&self) -> GoInt {
        self.message_id
    }

    pub fn event_type(&self) -> HttpEventType {
        self.event_type
    }

    pub fn method(&self) -> Option<&str> {
        self.method.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn client_ip(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.GetRequestClientIp(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetRequestClientIp", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn socket5_user(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.SunnyNetGetSocket5User(self.request_id) }
            .map_err(|err| SunnyNetError::operation_failed("SunnyNetGetSocket5User", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn request(&self) -> HttpRequestRef {
        HttpRequestRef::new(self.runtime.clone(), self.message_id)
    }

    pub fn response(&self) -> HttpResponseRef {
        HttpResponseRef::new(self.runtime.clone(), self.message_id)
    }
}

#[derive(Clone)]
pub struct TcpConnectionRef {
    runtime: SharedRuntime,
    unique_id: GoInt,
    message_id: GoInt,
    event_type: TcpEventType,
}

impl TcpConnectionRef {
    fn new(
        runtime: SharedRuntime,
        unique_id: GoInt,
        message_id: GoInt,
        event_type: TcpEventType,
    ) -> Self {
        Self {
            runtime,
            unique_id,
            message_id,
            event_type,
        }
    }

    pub fn set_out_router_ip(&self, ip: &str) -> SunnyNetResult<()> {
        let ip = cstring_arg("out_router_ip", ip)?;
        let success = unsafe {
            self.runtime
                .RequestSetOutRouterIP(self.message_id, ip.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("RequestSetOutRouterIP", err))?;
        success_or("RequestSetOutRouterIP", success != 0)
    }

    pub fn set_proxy(&self, proxy: &str, timeout_ms: u32) -> SunnyNetResult<()> {
        let proxy = cstring_arg("proxy", proxy)?;
        let success = unsafe {
            self.runtime
                .SetTcpAgent(self.message_id, proxy.as_ptr(), GoInt::from(timeout_ms))
        }
        .map_err(|err| SunnyNetError::operation_failed("SetTcpAgent", err))?;
        success_or("SetTcpAgent", success != 0)
    }

    pub fn redirect(&self, address: &str) -> SunnyNetResult<()> {
        let address = cstring_arg("address", address)?;
        let success = unsafe {
            self.runtime
                .SetTcpConnectionIP(self.message_id, address.as_ptr())
        }
        .map_err(|err| SunnyNetError::operation_failed("SetTcpConnectionIP", err))?;
        success_or("SetTcpConnectionIP", success != 0)
    }

    pub fn set_body(&self, body: &[u8]) -> SunnyNetResult<()> {
        let success = unsafe {
            self.runtime.SetTcpBody(
                self.message_id,
                GoInt::from(self.event_type.as_i32()),
                body.as_ptr() as usize,
                go_int_len(body.len(), "SetTcpBody")?,
            )
        }
        .map_err(|err| SunnyNetError::operation_failed("SetTcpBody", err))?;
        success_or("SetTcpBody", success != 0)
    }

    pub fn send(&self, target: TrafficTarget, body: &[u8]) -> SunnyNetResult<()> {
        let success = match target {
            TrafficTarget::Server => unsafe {
                self.runtime.TcpSendMsg(
                    self.unique_id,
                    body.as_ptr() as usize,
                    go_int_len(body.len(), "TcpSendMsg")?,
                )
            }
            .map_err(|err| SunnyNetError::operation_failed("TcpSendMsg", err))?,
            TrafficTarget::Client => unsafe {
                self.runtime.TcpSendMsgClient(
                    self.unique_id,
                    body.as_ptr() as usize,
                    go_int_len(body.len(), "TcpSendMsgClient")?,
                )
            }
            .map_err(|err| SunnyNetError::operation_failed("TcpSendMsgClient", err))?,
        };
        success_or("TcpSend", success != 0)
    }

    pub fn close(&self) -> SunnyNetResult<()> {
        let success = unsafe { self.runtime.TcpCloseClient(self.unique_id) }
            .map_err(|err| SunnyNetError::operation_failed("TcpCloseClient", err))?;
        success_or("TcpCloseClient", success != 0)
    }
}

#[derive(Clone)]
pub struct TcpEvent {
    runtime: SharedRuntime,
    context: GoInt,
    unique_id: GoInt,
    message_id: GoInt,
    event_type: TcpEventType,
    local_address: Option<String>,
    remote_address: Option<String>,
    body: Vec<u8>,
    pid: Option<u32>,
}

impl TcpEvent {
    pub(crate) fn from_raw(
        runtime: SharedRuntime,
        context: GoInt,
        local_ptr: *const std::ffi::c_char,
        remote_ptr: *const std::ffi::c_char,
        event_type: GoInt,
        message_id: GoInt,
        data_ptr: usize,
        data_len: GoInt,
        unique_id: GoInt,
        pid: GoInt,
    ) -> SunnyNetResult<Self> {
        Ok(Self {
            runtime,
            context,
            unique_id,
            message_id,
            event_type: TcpEventType::from_raw(event_type),
            local_address: read_callback_string(local_ptr),
            remote_address: read_callback_string(remote_ptr),
            body: read_borrowed_bytes(data_ptr, data_len)?,
            pid: u32::try_from(pid).ok().filter(|value| *value > 0),
        })
    }

    pub fn event_type(&self) -> TcpEventType {
        self.event_type
    }

    pub fn local_address(&self) -> Option<&str> {
        self.local_address.as_deref()
    }

    pub fn remote_address(&self) -> Option<&str> {
        self.remote_address.as_deref()
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn body_text(&self) -> Option<String> {
        if self.body.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&self.body).to_string())
        }
    }

    pub fn context(&self) -> GoInt {
        self.context
    }

    pub fn unique_id(&self) -> GoInt {
        self.unique_id
    }

    pub fn message_id(&self) -> GoInt {
        self.message_id
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn socket5_user(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.SunnyNetGetSocket5User(self.unique_id) }
            .map_err(|err| SunnyNetError::operation_failed("SunnyNetGetSocket5User", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn connection(&self) -> TcpConnectionRef {
        TcpConnectionRef::new(
            self.runtime.clone(),
            self.unique_id,
            self.message_id,
            self.event_type,
        )
    }
}
#[derive(Clone)]
pub struct WebSocketRef {
    runtime: SharedRuntime,
    unique_id: GoInt,
    message_id: GoInt,
}

impl WebSocketRef {
    fn new(runtime: SharedRuntime, unique_id: GoInt, message_id: GoInt) -> Self {
        Self {
            runtime,
            unique_id,
            message_id,
        }
    }

    pub fn body_len(&self) -> SunnyNetResult<usize> {
        let len = unsafe { self.runtime.GetWebsocketBodyLen(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetWebsocketBodyLen", err))?;
        if len <= 0 {
            Ok(0)
        } else {
            usize::try_from(len).map_err(|_| {
                SunnyNetError::operation_failed("GetWebsocketBodyLen", "invalid_length")
            })
        }
    }

    pub fn body(&self) -> SunnyNetResult<Vec<u8>> {
        let len = self.body_len()?;
        if len == 0 {
            return Ok(Vec::new());
        }
        let ptr = unsafe { self.runtime.GetWebsocketBody(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetWebsocketBody", err))?;
        read_owned_bytes(&self.runtime, ptr, len as GoInt)
    }

    pub fn text(&self) -> SunnyNetResult<Option<String>> {
        let body = self.body()?;
        if body.is_empty() {
            Ok(None)
        } else {
            Ok(Some(String::from_utf8_lossy(&body).to_string()))
        }
    }

    pub fn set_body(&self, body: &[u8]) -> SunnyNetResult<()> {
        let success = unsafe {
            self.runtime.SetWebsocketBody(
                self.message_id,
                body.as_ptr() as usize,
                go_int_len(body.len(), "SetWebsocketBody")?,
            )
        }
        .map_err(|err| SunnyNetError::operation_failed("SetWebsocketBody", err))?;
        success_or("SetWebsocketBody", success != 0)
    }

    pub fn send(
        &self,
        target: TrafficTarget,
        message_type: WebSocketMessageType,
        body: &[u8],
    ) -> SunnyNetResult<()> {
        let message_type = GoInt::from(message_type.as_i32());
        let success = match target {
            TrafficTarget::Server => unsafe {
                self.runtime.SendWebsocketClientBody(
                    self.unique_id,
                    message_type,
                    body.as_ptr() as usize,
                    go_int_len(body.len(), "SendWebsocketClientBody")?,
                )
            }
            .map_err(|err| SunnyNetError::operation_failed("SendWebsocketClientBody", err))?,
            TrafficTarget::Client => unsafe {
                self.runtime.SendWebsocketBody(
                    self.unique_id,
                    message_type,
                    body.as_ptr() as usize,
                    go_int_len(body.len(), "SendWebsocketBody")?,
                )
            }
            .map_err(|err| SunnyNetError::operation_failed("SendWebsocketBody", err))?,
        };
        success_or("SendWebsocket", success != 0)
    }

    pub fn close(&self) -> SunnyNetResult<()> {
        let success = unsafe { self.runtime.CloseWebsocket(self.unique_id) }
            .map_err(|err| SunnyNetError::operation_failed("CloseWebsocket", err))?;
        success_or("CloseWebsocket", success != 0)
    }
}

#[derive(Clone)]
pub struct WebSocketEvent {
    runtime: SharedRuntime,
    context: GoInt,
    unique_id: GoInt,
    message_id: GoInt,
    event_type: WebSocketEventType,
    message_type: WebSocketMessageType,
    method: Option<String>,
    url: Option<String>,
    pid: Option<u32>,
}

impl WebSocketEvent {
    pub(crate) fn from_raw(
        runtime: SharedRuntime,
        context: GoInt,
        unique_id: GoInt,
        message_id: GoInt,
        event_type: GoInt,
        method_ptr: *const std::ffi::c_char,
        url_ptr: *const std::ffi::c_char,
        pid: GoInt,
        message_type: GoInt,
    ) -> Self {
        Self {
            runtime,
            context,
            unique_id,
            message_id,
            event_type: WebSocketEventType::from_raw(event_type),
            message_type: WebSocketMessageType::from_raw(message_type),
            method: read_callback_string(method_ptr),
            url: read_callback_string(url_ptr),
            pid: u32::try_from(pid).ok().filter(|value| *value > 0),
        }
    }

    pub fn context(&self) -> GoInt {
        self.context
    }

    pub fn unique_id(&self) -> GoInt {
        self.unique_id
    }

    pub fn message_id(&self) -> GoInt {
        self.message_id
    }

    pub fn event_type(&self) -> WebSocketEventType {
        self.event_type
    }

    pub fn message_type(&self) -> WebSocketMessageType {
        self.message_type
    }

    pub fn method(&self) -> Option<&str> {
        self.method.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn client_ip(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.GetRequestClientIp(self.message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetRequestClientIp", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn socket5_user(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.SunnyNetGetSocket5User(self.unique_id) }
            .map_err(|err| SunnyNetError::operation_failed("SunnyNetGetSocket5User", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn request(&self) -> HttpRequestRef {
        HttpRequestRef::new(self.runtime.clone(), self.message_id)
    }

    pub fn socket(&self) -> WebSocketRef {
        WebSocketRef::new(self.runtime.clone(), self.unique_id, self.message_id)
    }
}

#[derive(Clone)]
pub struct UdpPacketRef {
    runtime: SharedRuntime,
    unique_id: GoInt,
    message_id: GoInt,
}

impl UdpPacketRef {
    fn new(runtime: SharedRuntime, unique_id: GoInt, message_id: GoInt) -> Self {
        Self {
            runtime,
            unique_id,
            message_id,
        }
    }

    pub fn set_body(&self, body: &[u8]) -> SunnyNetResult<()> {
        let success = unsafe {
            self.runtime.SetUdpData(
                self.message_id,
                body.as_ptr() as usize,
                go_int_len(body.len(), "SetUdpData")?,
            )
        }
        .map_err(|err| SunnyNetError::operation_failed("SetUdpData", err))?;
        success_or("SetUdpData", success != 0)
    }

    pub fn send(&self, target: TrafficTarget, body: &[u8]) -> SunnyNetResult<()> {
        let success = match target {
            TrafficTarget::Server => unsafe {
                self.runtime.UdpSendToServer(
                    self.unique_id,
                    body.as_ptr() as usize,
                    go_int_len(body.len(), "UdpSendToServer")?,
                )
            }
            .map_err(|err| SunnyNetError::operation_failed("UdpSendToServer", err))?,
            TrafficTarget::Client => unsafe {
                self.runtime.UdpSendToClient(
                    self.unique_id,
                    body.as_ptr() as usize,
                    go_int_len(body.len(), "UdpSendToClient")?,
                )
            }
            .map_err(|err| SunnyNetError::operation_failed("UdpSendToClient", err))?,
        };
        success_or("UdpSend", success != 0)
    }
}

#[derive(Clone)]
pub struct UdpEvent {
    runtime: SharedRuntime,
    context: GoInt,
    unique_id: GoInt,
    message_id: GoInt,
    event_type: UdpEventType,
    local_address: Option<String>,
    remote_address: Option<String>,
    body: Vec<u8>,
    pid: Option<u32>,
}

impl UdpEvent {
    pub(crate) fn from_raw(
        runtime: SharedRuntime,
        context: GoInt,
        local_ptr: *const std::ffi::c_char,
        remote_ptr: *const std::ffi::c_char,
        event_type: GoInt,
        message_id: GoInt,
        unique_id: GoInt,
        pid: GoInt,
    ) -> SunnyNetResult<Self> {
        let data_ptr = unsafe { runtime.GetUdpData(message_id) }
            .map_err(|err| SunnyNetError::operation_failed("GetUdpData", err))?;
        let body = read_len_prefixed_bytes(&runtime, data_ptr)?;
        Ok(Self {
            runtime,
            context,
            unique_id,
            message_id,
            event_type: UdpEventType::from_raw(event_type),
            local_address: read_callback_string(local_ptr),
            remote_address: read_callback_string(remote_ptr),
            body,
            pid: u32::try_from(pid).ok().filter(|value| *value > 0),
        })
    }

    pub fn event_type(&self) -> UdpEventType {
        self.event_type
    }

    pub fn local_address(&self) -> Option<&str> {
        self.local_address.as_deref()
    }

    pub fn remote_address(&self) -> Option<&str> {
        self.remote_address.as_deref()
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn body_text(&self) -> Option<String> {
        if self.body.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&self.body).to_string())
        }
    }

    pub fn context(&self) -> GoInt {
        self.context
    }

    pub fn unique_id(&self) -> GoInt {
        self.unique_id
    }

    pub fn message_id(&self) -> GoInt {
        self.message_id
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn socket5_user(&self) -> SunnyNetResult<Option<String>> {
        let ptr = unsafe { self.runtime.SunnyNetGetSocket5User(self.unique_id) }
            .map_err(|err| SunnyNetError::operation_failed("SunnyNetGetSocket5User", err))?;
        Ok(read_owned_string(&self.runtime, ptr))
    }

    pub fn packet(&self) -> UdpPacketRef {
        UdpPacketRef::new(self.runtime.clone(), self.unique_id, self.message_id)
    }
}

#[derive(Clone, Debug)]
pub struct ScriptLogEvent {
    context: GoInt,
    message: Option<String>,
}

impl ScriptLogEvent {
    pub(crate) fn from_raw(context: GoInt, message_ptr: *const std::ffi::c_char) -> Self {
        Self {
            context,
            message: read_callback_string(message_ptr),
        }
    }

    pub fn context(&self) -> GoInt {
        self.context
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[derive(Clone, Debug)]
pub struct ScriptSaveEvent {
    context: GoInt,
    code: String,
}

impl ScriptSaveEvent {
    pub(crate) fn from_raw(
        context: GoInt,
        code_ptr: usize,
        code_len: GoInt,
    ) -> SunnyNetResult<Self> {
        let bytes = read_borrowed_bytes(code_ptr, code_len)?;
        Ok(Self {
            context,
            code: String::from_utf8_lossy(&bytes).to_string(),
        })
    }

    pub fn context(&self) -> GoInt {
        self.context
    }

    pub fn code(&self) -> &str {
        &self.code
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HttpEventType, TcpEventType, UdpEventType, WebSocketEventType, WebSocketMessageType,
    };

    #[test]
    fn http_event_type_uses_cplusplus_constants() {
        assert_eq!(HttpEventType::from_raw(1), HttpEventType::Request);
        assert_eq!(HttpEventType::from_raw(2), HttpEventType::Response);
        assert_eq!(HttpEventType::from_raw(3), HttpEventType::Error);
    }

    #[test]
    fn tcp_event_type_uses_cplusplus_constants() {
        assert_eq!(TcpEventType::from_raw(4), TcpEventType::About);
        assert_eq!(TcpEventType::from_raw(0), TcpEventType::Connected);
        assert_eq!(TcpEventType::from_raw(3), TcpEventType::Close);
    }

    #[test]
    fn websocket_event_and_message_types_match_cplusplus_constants() {
        assert_eq!(
            WebSocketEventType::from_raw(1),
            WebSocketEventType::Connected
        );
        assert_eq!(WebSocketEventType::from_raw(4), WebSocketEventType::Close);
        assert_eq!(
            WebSocketMessageType::from_raw(1),
            WebSocketMessageType::Text
        );
        assert_eq!(
            WebSocketMessageType::from_raw(10),
            WebSocketMessageType::Pong
        );
    }

    #[test]
    fn udp_event_type_uses_cplusplus_constants() {
        assert_eq!(UdpEventType::from_raw(1), UdpEventType::Close);
        assert_eq!(UdpEventType::from_raw(2), UdpEventType::Send);
        assert_eq!(UdpEventType::from_raw(3), UdpEventType::Receive);
    }
}
