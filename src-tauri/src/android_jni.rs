// Android JNI bindings for Tauri
// This module provides the required JNI functions that the Android Activity expects

#[cfg(target_os = "android")]
use std::os::raw::c_void;

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn JNI_OnLoad(_vm: *mut c_void, _reserved: *mut c_void) -> i32 {
    // Return JNI_VERSION_1_6
    0x00010006
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_create(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    // Activity created - initialize Tauri app here if needed
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_start(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    // Activity started
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_resume(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    // Activity resumed
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_pause(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    // Activity paused
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_stop(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    // Activity stopped
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_save(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    // Save instance state
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_destroy(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    // Activity destroyed - cleanup
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_onActivityDestroy(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    // Activity destroy callback
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_memory(
    _env: *mut c_void,
    _activity: *mut c_void,
) {
    // Low memory warning
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_claude_desktop_WryActivity_focus(
    _env: *mut c_void,
    _activity: *mut c_void,
    _focus: bool,
) {
    // Window focus changed
}
