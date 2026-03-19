# sunnynet-rs

Rust FFI wrapper for SunnyNet DLL, with dynamic symbol loading and a typed API entry (`LoadedSunnyNet`).

## What's included

- Rust crate source code (currently package name is `sunnynet-sdk` for compatibility)
- `bin/SunnyNet.dll`
- `bin/SunnyNet64.dll`

## Quick start

1. Add dependency in `Cargo.toml`:

Via crates.io:

```toml
[dependencies]
sunnynet-sdk = "0.1"
```

Or via GitHub:

```toml
[dependencies]
sunnynet-sdk = { git = "https://github.com/itisl2220/sunnynet-rs.git" }
```

2. Load DLL and check version:

```rust
use sunnynet_sdk::LoadedSunnyNet;
use std::path::Path;

fn main() -> Result<(), String> {
    let dll_path = Path::new("./bin/SunnyNet64.dll");
    let sdk = LoadedSunnyNet::load(dll_path)?;

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

    // ... do your capture / proxy work ...

    api.sunny_net_close(ctx);
    api.release_sunny_net(ctx);
    Ok(())
}
```

## Symbol compatibility check

```rust
let missing = sdk.missing_native_symbols();
if !missing.is_empty() {
    eprintln!("missing symbols: {missing:?}");
}
```

## Notes

- Current DLL package is Windows-only.
- Prefer `SunnyNet64.dll` on x64 environment.
- Crates.io package excludes DLL files; download from this repo's `bin/` directory.
- If you copy DLL to other locations, pass the actual path to `LoadedSunnyNet::load`.
