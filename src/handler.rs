use crate::events::{
    HttpEvent, ScriptLogEvent, ScriptSaveEvent, TcpEvent, UdpEvent, WebSocketEvent,
};

/// HTTP 事件处理器。
pub trait HttpHandler: Send + Sync {
    /// 处理 HTTP 事件。
    fn on_http_event(&self, _event: &mut HttpEvent) {}
}

/// TCP 事件处理器。
pub trait TcpHandler: Send + Sync {
    /// 处理 TCP 事件。
    fn on_tcp_event(&self, _event: &mut TcpEvent) {}
}

/// WebSocket 事件处理器。
pub trait WebSocketHandler: Send + Sync {
    /// 处理 WebSocket 事件。
    fn on_websocket_event(&self, _event: &mut WebSocketEvent) {}
}

/// UDP 事件处理器。
pub trait UdpHandler: Send + Sync {
    /// 处理 UDP 事件。
    fn on_udp_event(&self, _event: &mut UdpEvent) {}
}

/// 脚本事件处理器。
pub trait ScriptHandler: Send + Sync {
    /// 脚本日志回调。
    fn on_script_log(&self, _event: &ScriptLogEvent) {}

    /// 脚本保存回调。
    fn on_script_save(&self, _event: &ScriptSaveEvent) {}
}

/// 统一事件处理器约束。
pub trait SunnyNetHandler:
    HttpHandler + TcpHandler + WebSocketHandler + UdpHandler + ScriptHandler + Send + Sync + 'static
{
}

impl<T> SunnyNetHandler for T where
    T: HttpHandler
        + TcpHandler
        + WebSocketHandler
        + UdpHandler
        + ScriptHandler
        + Send
        + Sync
        + 'static
{
}

/// 空实现处理器，适合作为默认占位。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopHandler;

impl HttpHandler for NoopHandler {}
impl TcpHandler for NoopHandler {}
impl WebSocketHandler for NoopHandler {}
impl UdpHandler for NoopHandler {}
impl ScriptHandler for NoopHandler {}
