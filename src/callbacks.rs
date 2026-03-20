use crate::{
    events::{HttpEvent, ScriptLogEvent, ScriptSaveEvent, TcpEvent, UdpEvent, WebSocketEvent},
    handler::SunnyNetHandler,
    internal::{callback_addr, SharedRuntime},
    GoInt, SunnyNetError, SunnyNetResult,
};
use std::{
    collections::HashMap,
    ffi::c_char,
    sync::{Arc, Mutex, OnceLock},
};

trait CallbackDispatcher: Send + Sync {
    fn on_http(
        &self,
        context: GoInt,
        request_id: GoInt,
        message_id: GoInt,
        event_type: GoInt,
        method_ptr: *const c_char,
        url_ptr: *const c_char,
        error_ptr: *const c_char,
        pid: GoInt,
    );

    fn on_tcp(
        &self,
        context: GoInt,
        local_ptr: *const c_char,
        remote_ptr: *const c_char,
        event_type: GoInt,
        message_id: GoInt,
        data_ptr: usize,
        data_len: GoInt,
        request_id: GoInt,
        pid: GoInt,
    );

    fn on_websocket(
        &self,
        context: GoInt,
        request_id: GoInt,
        message_id: GoInt,
        event_type: GoInt,
        method_ptr: *const c_char,
        url_ptr: *const c_char,
        pid: GoInt,
        ws_message_type: GoInt,
    );

    fn on_udp(
        &self,
        context: GoInt,
        local_ptr: *const c_char,
        remote_ptr: *const c_char,
        event_type: GoInt,
        message_id: GoInt,
        request_id: GoInt,
        pid: GoInt,
    );

    fn on_script_log(&self, context: GoInt, message_ptr: *const c_char);

    fn on_script_save(&self, context: GoInt, code_ptr: usize, code_len: GoInt);
}

struct HandlerDispatcher<H> {
    runtime: SharedRuntime,
    handler: Arc<H>,
}

impl<H> HandlerDispatcher<H> {
    fn new(runtime: SharedRuntime, handler: Arc<H>) -> Self {
        Self { runtime, handler }
    }
}

impl<H> CallbackDispatcher for HandlerDispatcher<H>
where
    H: SunnyNetHandler,
{
    fn on_http(
        &self,
        context: GoInt,
        request_id: GoInt,
        message_id: GoInt,
        event_type: GoInt,
        method_ptr: *const c_char,
        url_ptr: *const c_char,
        error_ptr: *const c_char,
        pid: GoInt,
    ) {
        let mut event = HttpEvent::from_raw(
            self.runtime.clone(),
            context,
            request_id,
            message_id,
            event_type,
            method_ptr,
            url_ptr,
            error_ptr,
            pid,
        );
        self.handler.on_http_event(&mut event);
    }

    fn on_tcp(
        &self,
        context: GoInt,
        local_ptr: *const c_char,
        remote_ptr: *const c_char,
        event_type: GoInt,
        message_id: GoInt,
        data_ptr: usize,
        data_len: GoInt,
        request_id: GoInt,
        pid: GoInt,
    ) {
        if let Ok(mut event) = TcpEvent::from_raw(
            self.runtime.clone(),
            context,
            local_ptr,
            remote_ptr,
            event_type,
            message_id,
            data_ptr,
            data_len,
            request_id,
            pid,
        ) {
            self.handler.on_tcp_event(&mut event);
        }
    }

    fn on_websocket(
        &self,
        context: GoInt,
        request_id: GoInt,
        message_id: GoInt,
        event_type: GoInt,
        method_ptr: *const c_char,
        url_ptr: *const c_char,
        pid: GoInt,
        ws_message_type: GoInt,
    ) {
        let mut event = WebSocketEvent::from_raw(
            self.runtime.clone(),
            context,
            request_id,
            message_id,
            event_type,
            method_ptr,
            url_ptr,
            pid,
            ws_message_type,
        );
        self.handler.on_websocket_event(&mut event);
    }

    fn on_udp(
        &self,
        context: GoInt,
        local_ptr: *const c_char,
        remote_ptr: *const c_char,
        event_type: GoInt,
        message_id: GoInt,
        request_id: GoInt,
        pid: GoInt,
    ) {
        if let Ok(mut event) = UdpEvent::from_raw(
            self.runtime.clone(),
            context,
            local_ptr,
            remote_ptr,
            event_type,
            message_id,
            request_id,
            pid,
        ) {
            self.handler.on_udp_event(&mut event);
        }
    }

    fn on_script_log(&self, context: GoInt, message_ptr: *const c_char) {
        let event = ScriptLogEvent::from_raw(context, message_ptr);
        self.handler.on_script_log(&event);
    }

    fn on_script_save(&self, context: GoInt, code_ptr: usize, code_len: GoInt) {
        if let Ok(event) = ScriptSaveEvent::from_raw(context, code_ptr, code_len) {
            self.handler.on_script_save(&event);
        }
    }
}

fn registry() -> &'static Mutex<HashMap<GoInt, Arc<dyn CallbackDispatcher>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<GoInt, Arc<dyn CallbackDispatcher>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn dispatcher_for(context: GoInt) -> Option<Arc<dyn CallbackDispatcher>> {
    registry()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .get(&context)
        .cloned()
}

pub(crate) struct CallbackAddresses {
    pub http: GoInt,
    pub tcp: GoInt,
    pub websocket: GoInt,
    pub udp: GoInt,
    pub script_log: usize,
    pub script_save: usize,
}

impl CallbackAddresses {
    pub(crate) fn new() -> SunnyNetResult<Self> {
        Ok(Self {
            http: callback_addr(http_callback as *const () as usize)?,
            tcp: callback_addr(tcp_callback as *const () as usize)?,
            websocket: callback_addr(websocket_callback as *const () as usize)?,
            udp: callback_addr(udp_callback as *const () as usize)?,
            script_log: script_log_callback as *const () as usize,
            script_save: script_save_callback as *const () as usize,
        })
    }
}

pub(crate) fn register_handler<H>(
    runtime: SharedRuntime,
    context: GoInt,
    handler: Arc<H>,
) -> SunnyNetResult<()>
where
    H: SunnyNetHandler,
{
    let mut registry = registry().lock().unwrap_or_else(|err| err.into_inner());
    if registry.contains_key(&context) {
        return Err(SunnyNetError::operation_failed(
            "register_handler",
            "context_already_registered",
        ));
    }
    registry.insert(context, Arc::new(HandlerDispatcher::new(runtime, handler)));
    Ok(())
}

pub(crate) fn unregister_handler(context: GoInt) {
    registry()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .remove(&context);
}

unsafe extern "C" fn http_callback(
    context: GoInt,
    request_id: GoInt,
    message_id: GoInt,
    event_type: GoInt,
    method_ptr: *const c_char,
    url_ptr: *const c_char,
    error_ptr: *const c_char,
    pid: GoInt,
) {
    if let Some(dispatcher) = dispatcher_for(context) {
        dispatcher.on_http(
            context, request_id, message_id, event_type, method_ptr, url_ptr, error_ptr, pid,
        );
    }
}

unsafe extern "C" fn tcp_callback(
    context: GoInt,
    local_ptr: *const c_char,
    remote_ptr: *const c_char,
    event_type: GoInt,
    message_id: GoInt,
    data_ptr: usize,
    data_len: GoInt,
    request_id: GoInt,
    pid: GoInt,
) {
    if let Some(dispatcher) = dispatcher_for(context) {
        dispatcher.on_tcp(
            context, local_ptr, remote_ptr, event_type, message_id, data_ptr, data_len, request_id,
            pid,
        );
    }
}

unsafe extern "C" fn websocket_callback(
    context: GoInt,
    request_id: GoInt,
    message_id: GoInt,
    event_type: GoInt,
    method_ptr: *const c_char,
    url_ptr: *const c_char,
    pid: GoInt,
    ws_message_type: GoInt,
) {
    if let Some(dispatcher) = dispatcher_for(context) {
        dispatcher.on_websocket(
            context,
            request_id,
            message_id,
            event_type,
            method_ptr,
            url_ptr,
            pid,
            ws_message_type,
        );
    }
}

unsafe extern "C" fn udp_callback(
    context: GoInt,
    local_ptr: *const c_char,
    remote_ptr: *const c_char,
    event_type: GoInt,
    message_id: GoInt,
    request_id: GoInt,
    pid: GoInt,
) {
    if let Some(dispatcher) = dispatcher_for(context) {
        dispatcher.on_udp(
            context, local_ptr, remote_ptr, event_type, message_id, request_id, pid,
        );
    }
}

unsafe extern "C" fn script_log_callback(context: GoInt, message_ptr: *const c_char) {
    if let Some(dispatcher) = dispatcher_for(context) {
        dispatcher.on_script_log(context, message_ptr);
    }
}

unsafe extern "C" fn script_save_callback(context: GoInt, code_ptr: usize, code_len: GoInt) {
    if let Some(dispatcher) = dispatcher_for(context) {
        dispatcher.on_script_save(context, code_ptr, code_len);
    }
}

#[cfg(test)]
mod tests {
    use crate::handler::NoopHandler;

    #[test]
    fn noop_handler_is_valid_sunnynet_handler() {
        fn assert_handler<T: crate::handler::SunnyNetHandler>(_handler: T) {}
        assert_handler(NoopHandler);
    }
}
