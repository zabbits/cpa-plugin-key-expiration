use std::{
    ffi::{CStr, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    sync::OnceLock,
};

use serde_json::{Value, json};

use crate::{
    Plugin,
    error::{Error, INVALID_REQUEST},
    intercept,
};

#[repr(C)]
pub struct Buffer {
    ptr: *mut u8,
    len: usize,
}

type HostCall =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const u8, usize, *mut Buffer) -> i32;
type Free = unsafe extern "C" fn(*mut c_void, usize);
type Call = unsafe extern "C" fn(*const c_char, *const u8, usize, *mut Buffer) -> i32;

#[repr(C)]
pub struct HostApi {
    abi_version: u32,
    host_ctx: *mut c_void,
    call: Option<HostCall>,
    free_buffer: Option<Free>,
}

#[repr(C)]
pub struct PluginApi {
    abi_version: u32,
    call: Option<Call>,
    free_buffer: Option<Free>,
    shutdown: Option<unsafe extern "C" fn()>,
}

static PLUGIN: OnceLock<Plugin> = OnceLock::new();

/// # Safety
/// `host` and `plugin` must point to valid ABI v1 structures for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cliproxy_plugin_init(host: *const HostApi, plugin: *mut PluginApi) -> i32 {
    if host.is_null() || plugin.is_null() {
        return 1;
    }
    // SAFETY: The host owns both structures and supplies their valid pointers.
    unsafe {
        if (*host).abi_version != 1 {
            return 1;
        }
        ptr::write(
            plugin,
            PluginApi {
                abi_version: 1,
                call: Some(call),
                free_buffer: Some(free),
                shutdown: Some(shutdown),
            },
        );
    }
    0
}

unsafe extern "C" fn call(
    method: *const c_char,
    request: *const u8,
    len: usize,
    response: *mut Buffer,
) -> i32 {
    if response.is_null() {
        return 1;
    }
    // SAFETY: The host provides a writable output buffer and readable input buffers.
    unsafe {
        ptr::write(
            response,
            Buffer {
                ptr: ptr::null_mut(),
                len: 0,
            },
        );
    }
    let method = if method.is_null() {
        None
    } else {
        // SAFETY: The ABI supplies a NUL-terminated method string.
        unsafe { CStr::from_ptr(method) }.to_str().ok()
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let method = method.ok_or(INVALID_REQUEST)?;
        if request.is_null() && len != 0 {
            return Err(INVALID_REQUEST);
        }
        let bytes = if len == 0 {
            b"{}".as_slice()
        } else {
            // SAFETY: The input buffer remains owned by the host for the call duration.
            unsafe { slice::from_raw_parts(request, len) }
        };
        let request: Value = serde_json::from_slice(bytes).map_err(|_| INVALID_REQUEST)?;
        PLUGIN.get_or_init(Plugin::default).call(method, request)
    }));
    let result =
        outcome.unwrap_or_else(|_| Err(Error::new(500, "plugin_error", "Plugin request failed")));
    // Interceptor errors are otherwise ignored by CPA. Always return an explicit
    // rejection, including malformed requests and caught Rust panics.
    let result = match (method, result) {
        (Some("request.intercept_before"), Err(error)) => Ok(intercept::reject(error)),
        (Some("request.intercept_after"), _) => Ok(json!({})),
        (_, result) => result,
    };
    let envelope = match result {
        Ok(result) => json!({"ok": true, "result": result}),
        Err(error) => {
            json!({"ok": false, "error": {"code": error.code, "message": error.message, "http_status": error.status}})
        }
    };
    let mut bytes = envelope.to_string().into_bytes().into_boxed_slice();
    let buffer = Buffer {
        ptr: bytes.as_mut_ptr(),
        len: bytes.len(),
    };
    std::mem::forget(bytes);
    // SAFETY: Ownership transfers to the host until it invokes our free callback.
    unsafe {
        ptr::write(response, buffer);
    }
    0
}

unsafe extern "C" fn free(buffer: *mut c_void, len: usize) {
    if !buffer.is_null() {
        // SAFETY: This must be a buffer returned by `call`, freed exactly once.
        unsafe {
            drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
                buffer.cast::<u8>(),
                len,
            )));
        }
    }
}

unsafe extern "C" fn shutdown() {
    let _ = catch_unwind(|| {
        if let Some(plugin) = PLUGIN.get() {
            plugin.shutdown();
        }
    });
}
