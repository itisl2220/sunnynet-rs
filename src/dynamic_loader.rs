use libloading::Library;
use std::ffi::c_void;

pub fn load_required<T>(library: &Library, symbol_name: &str) -> Result<T, String>
where
    T: Copy,
{
    let symbol = symbol_name_to_bytes(symbol_name);
    unsafe {
        Ok(*library.get::<T>(&symbol).map_err(|err| {
            format!(
                "load_symbol_failed {}: {err}",
                String::from_utf8_lossy(&symbol)
            )
        })?)
    }
}

pub fn load_optional<T>(library: &Library, symbol_name: &str) -> Option<T>
where
    T: Copy,
{
    let symbol = symbol_name_to_bytes(symbol_name);
    unsafe { library.get::<T>(&symbol).ok().map(|value| *value) }
}

pub fn has_symbol(library: &Library, symbol_name: &str) -> bool {
    let symbol = symbol_name_to_bytes(symbol_name);
    unsafe { library.get::<*const c_void>(&symbol).is_ok() }
}

pub unsafe fn resolve_symbol<T>(library: &Library, symbol_name: &str) -> Result<T, String>
where
    T: Copy,
{
    load_required(library, symbol_name)
}

fn symbol_name_to_bytes(symbol_name: &str) -> Vec<u8> {
    let mut bytes = symbol_name.as_bytes().to_vec();
    if !bytes.ends_with(&[0]) {
        bytes.push(0);
    }
    bytes
}
