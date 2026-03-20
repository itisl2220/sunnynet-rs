# sunnynet-sdk

`sunnynet-sdk` 是 SunnyNet DLL 的 Rust FFI 封装，提供两层能力：

- `LoadedSunnyNet`：低层符号加载 + 原生函数访问，适合需要完整控制的场景。
- `SunnyNetClient`：高层客户端封装，内置回调注册、状态管理、证书与系统代理操作。

## 功能概览

- 动态加载 `SunnyNet.dll` / `SunnyNet64.dll`
- 暴露常用 SunnyNet API（启动、停止、端口设置、证书安装等）
- 提供 HTTP/TCP/WS/UDP 事件回调 trait
- 支持导出符号缺失检查，方便排查 DLL 版本兼容问题

## 运行环境

- 平台：Windows（当前 DLL 仅提供 Windows 版本）
- Rust：Edition 2021
- 推荐：x64 系统使用 `SunnyNet64.dll`

## 安装

### 方式一：crates.io

```toml
[dependencies]
sunnynet-sdk = "0.1"
```

### 方式二：GitHub

```toml
[dependencies]
sunnynet-sdk = { git = "https://github.com/itisl2220/sunnynet-rs.git" }
```

## 快速开始（低层 API）

```rust
use std::path::Path;
use sunnynet_sdk::LoadedSunnyNet;

fn main() -> Result<(), String> {
    let sdk = LoadedSunnyNet::load(Path::new("./bin/SunnyNet64.dll"))?;
    println!("SunnyNet version: {:?}", sdk.get_version());

    let api = sdk.api();
    let ctx = api.create_sunny_net();
    if ctx == 0 {
        return Err("create_sunny_net_failed".to_string());
    }

    if !api.sunny_net_set_port(ctx, 2024) {
        return Err("sunny_net_set_port_failed".to_string());
    }

    if !api.sunny_net_start(ctx) {
        let detail = api
            .read_owned_string(api.sunny_net_error_ptr(ctx))
            .unwrap_or_else(|| "unknown error".to_string());
        return Err(format!("sunny_net_start_failed: {detail}"));
    }

    // ... 业务逻辑 ...

    api.sunny_net_close(ctx);
    api.release_sunny_net(ctx);
    Ok(())
}
```

## 快速开始（高层 Client）

```rust
use sunnynet_sdk::{HttpHandler, HttpEvent, NoopHandler, SunnyNetClient};

#[derive(Default)]
struct MyHandler;

impl HttpHandler for MyHandler {
    fn on_http_event(&self, event: &mut HttpEvent) {
        let _ = event.request().and_then(|req| req.url());
    }
}

// 其余事件不关心时，可直接继承 NoopHandler 的空实现
impl sunnynet_sdk::TcpHandler for MyHandler {}
impl sunnynet_sdk::WebSocketHandler for MyHandler {}
impl sunnynet_sdk::UdpHandler for MyHandler {}
impl sunnynet_sdk::ScriptHandler for MyHandler {}

fn build_client() -> Result<SunnyNetClient<MyHandler>, sunnynet_sdk::SunnyNetError> {
    SunnyNetClient::builder(MyHandler::default())
        .port(20250)
        .build()
}
```

## DLL 兼容性检查

```rust
let missing = sdk.missing_native_symbols();
if !missing.is_empty() {
    eprintln!("missing symbols: {missing:?}");
}
```

## 目录说明

- `src/lib.rs`：底层 API 装载、符号解析入口
- `src/client.rs`：高层 `SunnyNetClient` 封装
- `src/events.rs`：事件对象与请求/响应访问封装
- `src/handler.rs`：事件处理 trait 定义
- `src/cert.rs`：证书管理与安装结果封装
- `src/native_symbols.rs`：导出符号清单
- `bin/`：示例 DLL（发布到 crates.io 时默认排除）

## 常见注意事项

- crates.io 发布包默认不包含 DLL，生产环境请自行分发或在部署时下载。
- `SunnyNetInstallCert` 在部分 DLL 版本中可能不存在，调用前请确认支持。
- 所有由 DLL 分配并返回的字符串指针，都必须通过 SDK 内部 `Free` 回收。

## License

MIT
