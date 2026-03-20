use crate::{GoInt, GoUint8, GoUintptr, LoadedSunnyNet, SunnyNetError, SunnyNetResult};
use std::{
    ffi::{c_char, CStr, CString},
    sync::Arc,
};

pub(crate) type SharedRuntime = Arc<LoadedSunnyNet>;

pub(crate) fn cstring_arg(field: &'static str, value: &str) -> SunnyNetResult<CString> {
    CString::new(value)
        .map_err(|err| SunnyNetError::invalid_argument(field, format!("cstring_failed: {err}")))
}

pub(crate) fn bool_to_flag(value: bool) -> GoUint8 {
    if value {
        1
    } else {
        0
    }
}

pub(crate) fn callback_addr(value: usize) -> SunnyNetResult<GoInt> {
    GoInt::try_from(value)
        .map_err(|_| SunnyNetError::operation_failed("callback_addr", "address_out_of_range"))
}

pub(crate) fn read_callback_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let value = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub(crate) fn read_owned_string(runtime: &LoadedSunnyNet, ptr: GoUintptr) -> Option<String> {
    runtime.read_owned_string(ptr)
}

pub(crate) fn read_owned_bytes(
    runtime: &LoadedSunnyNet,
    ptr: GoUintptr,
    len: GoInt,
) -> SunnyNetResult<Vec<u8>> {
    if ptr == 0 || len <= 0 {
        return Ok(Vec::new());
    }

    let len = usize::try_from(len)
        .map_err(|_| SunnyNetError::operation_failed("read_owned_bytes", "invalid_length"))?;
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len).to_vec() };
    runtime.api().free(ptr);
    Ok(bytes)
}

pub(crate) fn read_len_prefixed_bytes(
    runtime: &LoadedSunnyNet,
    ptr: GoUintptr,
) -> SunnyNetResult<Vec<u8>> {
    if ptr == 0 {
        return Ok(Vec::new());
    }

    let len = unsafe { runtime.BytesToInt(ptr, 8) }
        .map_err(|err| SunnyNetError::operation_failed("BytesToInt", err))?;
    let len = usize::try_from(len).map_err(|_| {
        SunnyNetError::operation_failed("read_len_prefixed_bytes", "invalid_length")
    })?;
    let bytes = unsafe { std::slice::from_raw_parts((ptr + 8) as *const u8, len).to_vec() };
    runtime.api().free(ptr);
    Ok(bytes)
}

pub(crate) fn read_borrowed_bytes(ptr: GoUintptr, len: GoInt) -> SunnyNetResult<Vec<u8>> {
    if ptr == 0 || len <= 0 {
        return Ok(Vec::new());
    }

    let len = usize::try_from(len)
        .map_err(|_| SunnyNetError::operation_failed("read_borrowed_bytes", "invalid_length"))?;
    Ok(unsafe { std::slice::from_raw_parts(ptr as *const u8, len).to_vec() })
}

pub(crate) fn split_header_values(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn join_header_values(values: &[String]) -> String {
    values.join("\n")
}

pub(crate) fn success_or(operation: &'static str, success: bool) -> SunnyNetResult<()> {
    if success {
        Ok(())
    } else {
        Err(SunnyNetError::operation_failed(
            operation,
            "native_call_returned_false",
        ))
    }
}

pub(crate) fn go_int_len(len: usize, operation: &'static str) -> SunnyNetResult<GoInt> {
    GoInt::try_from(len)
        .map_err(|_| SunnyNetError::operation_failed(operation, "length_out_of_range"))
}

pub(crate) fn string_first_value(value: String) -> Option<String> {
    split_header_values(&value).into_iter().next()
}
