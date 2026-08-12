/// Optional native Frida built-ins.
///
/// The module is always registered so scripts receive a useful error from a
/// normal build. Its FFI implementation is compiled only with `--features frida`.
use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

#[cfg(feature = "frida")]
use std::collections::VecDeque;
#[cfg(feature = "frida")]
use std::ffi::{CStr, CString};
#[cfg(feature = "frida")]
use std::os::raw::{c_char, c_void};
#[cfg(feature = "frida")]
use std::sync::{Condvar, Mutex, OnceLock};
#[cfg(feature = "frida")]
use std::time::{Duration, Instant};

#[cfg(feature = "frida")]
use frida_sys::*;

#[cfg(not(feature = "frida"))]
const DISABLED_MESSAGE: &str = "this legible binary was built without Frida support; rebuild with `cargo install --path . --features frida`";

fn frida_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

#[cfg(not(feature = "frida"))]
fn disabled_error() -> LegibleError {
    frida_error(
        DISABLED_MESSAGE,
        "Install a Legible binary with the optional `frida` feature",
    )
}

fn require_arity(args: &[Value], name: &str, count: usize) -> Result<(), LegibleError> {
    if args.len() == count {
        Ok(())
    } else {
        Err(frida_error(
            &format!(
                "{name}() expects {count} argument{}",
                if count == 1 { "" } else { "s" }
            ),
            &format!("Usage: {name}(...)"),
        ))
    }
}

fn expect_integer(args: &[Value], index: usize, name: &str) -> Result<i64, LegibleError> {
    match args.get(index) {
        Some(Value::Integer(value)) => Ok(*value),
        _ => Err(frida_error(
            &format!("{name}() expects an integer argument"),
            "Pass integer arguments",
        )),
    }
}

fn expect_text<'a>(args: &'a [Value], index: usize, name: &str) -> Result<&'a str, LegibleError> {
    match args.get(index) {
        Some(Value::Text(value)) => Ok(value),
        _ => Err(frida_error(
            &format!("{name}() expects a text argument"),
            "Pass text arguments",
        )),
    }
}

/// Register Frida functions, including useful stubs in non-Frida binaries.
pub fn register_frida_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("frida_version", builtin_frida_version),
        ("frida_device_ids", builtin_frida_device_ids),
        ("frida_device_name", builtin_frida_device_name),
        ("frida_open_device", builtin_frida_open_device),
        ("frida_usb_device", builtin_frida_usb_device),
        (
            "frida_device_process_names",
            builtin_frida_device_process_names,
        ),
        ("frida_device_process_pid", builtin_frida_device_process_pid),
        ("frida_spawn", builtin_frida_spawn),
        ("frida_resume", builtin_frida_resume),
        ("frida_kill", builtin_frida_kill),
        ("frida_attach", builtin_frida_attach),
        ("frida_detach", builtin_frida_detach),
        ("frida_create_script", builtin_frida_create_script),
        ("frida_load_script", builtin_frida_load_script),
        ("frida_unload_script", builtin_frida_unload_script),
        ("frida_next_message", builtin_frida_next_message),
        ("frida_wait_message", builtin_frida_wait_message),
    ];

    for (name, func) in builtins {
        env.borrow_mut().define(
            name.to_string(),
            Value::Function(Callable::Builtin {
                name: name.to_string(),
                func,
            }),
            false,
        );
    }
}

#[cfg(feature = "frida")]
struct DeviceHandle {
    ptr: *mut FridaDevice,
}
#[cfg(feature = "frida")]
unsafe impl Send for DeviceHandle {}

#[cfg(feature = "frida")]
struct SessionHandle {
    ptr: *mut FridaSession,
}
#[cfg(feature = "frida")]
unsafe impl Send for SessionHandle {}

#[cfg(feature = "frida")]
struct ScriptEntry {
    ptr: *mut FridaScript,
    messages: VecDeque<String>,
}
#[cfg(feature = "frida")]
unsafe impl Send for ScriptEntry {}

#[cfg(feature = "frida")]
struct DeviceManagerHandle {
    ptr: *mut FridaDeviceManager,
}
#[cfg(feature = "frida")]
unsafe impl Send for DeviceManagerHandle {}

#[cfg(feature = "frida")]
static FRIDA_INIT: OnceLock<()> = OnceLock::new();
#[cfg(feature = "frida")]
static DEVICE_MANAGER: OnceLock<Mutex<Option<DeviceManagerHandle>>> = OnceLock::new();
#[cfg(feature = "frida")]
static DEVICES: OnceLock<Mutex<Vec<Option<DeviceHandle>>>> = OnceLock::new();
#[cfg(feature = "frida")]
static SESSIONS: OnceLock<Mutex<Vec<Option<SessionHandle>>>> = OnceLock::new();
#[cfg(feature = "frida")]
static SCRIPTS: OnceLock<(Mutex<Vec<Option<ScriptEntry>>>, Condvar)> = OnceLock::new();

#[cfg(feature = "frida")]
fn devices() -> &'static Mutex<Vec<Option<DeviceHandle>>> {
    DEVICES.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(feature = "frida")]
fn sessions() -> &'static Mutex<Vec<Option<SessionHandle>>> {
    SESSIONS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(feature = "frida")]
fn scripts() -> &'static (Mutex<Vec<Option<ScriptEntry>>>, Condvar) {
    SCRIPTS.get_or_init(|| (Mutex::new(Vec::new()), Condvar::new()))
}

#[cfg(feature = "frida")]
fn ensure_frida() {
    FRIDA_INIT.get_or_init(|| unsafe { frida_init() });
}

#[cfg(feature = "frida")]
fn manager() -> Result<*mut FridaDeviceManager, LegibleError> {
    ensure_frida();
    let manager = DEVICE_MANAGER.get_or_init(|| Mutex::new(None));
    let mut guard = manager.lock().map_err(|_| {
        frida_error(
            "Frida device manager is unavailable",
            "Try again in a new process",
        )
    })?;
    if guard.is_none() {
        let ptr = unsafe { frida_device_manager_new() };
        if ptr.is_null() {
            return Err(frida_error(
                "Failed to create Frida device manager",
                "Check that Frida is installed correctly",
            ));
        }
        *guard = Some(DeviceManagerHandle { ptr });
    }
    match guard.as_ref() {
        Some(manager) => Ok(manager.ptr),
        None => Err(frida_error(
            "Frida device manager could not be initialized",
            "Try again in a new process",
        )),
    }
}

#[cfg(feature = "frida")]
unsafe fn take_gerror(error: *mut GError) -> Option<String> {
    if error.is_null() {
        return None;
    }
    let message = if (*error).message.is_null() {
        "Frida returned an unspecified error".to_string()
    } else {
        CStr::from_ptr((*error).message)
            .to_string_lossy()
            .into_owned()
    };
    _frida_g_error_free(error);
    Some(message)
}

#[cfg(feature = "frida")]
fn ffi_error(operation: &str, error: *mut GError) -> Result<(), LegibleError> {
    match unsafe { take_gerror(error) } {
        Some(message) => Err(frida_error(
            &format!("{operation}: {message}"),
            "Check the device, process, and Frida connection",
        )),
        None => Ok(()),
    }
}

#[cfg(feature = "frida")]
fn c_string(value: &str, name: &str) -> Result<CString, LegibleError> {
    CString::new(value).map_err(|_| {
        frida_error(
            &format!("{name}() text cannot contain a NUL byte"),
            "Remove the NUL byte from the text argument",
        )
    })
}

#[cfg(feature = "frida")]
fn handle_index(handle: i64, kind: &str) -> Result<usize, LegibleError> {
    if handle <= 0 {
        return Err(frida_error(
            &format!("Invalid {kind} handle"),
            "Use a handle returned by a Frida builtin",
        ));
    }
    usize::try_from(handle - 1).map_err(|_| {
        frida_error(
            &format!("Invalid {kind} handle"),
            "Use a handle returned by a Frida builtin",
        )
    })
}

#[cfg(feature = "frida")]
fn device_ptr(handle: i64) -> Result<*mut FridaDevice, LegibleError> {
    let index = handle_index(handle, "device")?;
    let guard = devices().lock().map_err(|_| {
        frida_error(
            "Frida device registry is unavailable",
            "Try again in a new process",
        )
    })?;
    guard
        .get(index)
        .and_then(Option::as_ref)
        .map(|device| device.ptr)
        .ok_or_else(|| {
            frida_error(
                "Unknown device handle",
                "Use a live handle returned by frida_open_device() or frida_usb_device()",
            )
        })
}

#[cfg(feature = "frida")]
fn session_ptr(handle: i64) -> Result<*mut FridaSession, LegibleError> {
    let index = handle_index(handle, "session")?;
    let guard = sessions().lock().map_err(|_| {
        frida_error(
            "Frida session registry is unavailable",
            "Try again in a new process",
        )
    })?;
    guard
        .get(index)
        .and_then(Option::as_ref)
        .map(|session| session.ptr)
        .ok_or_else(|| {
            frida_error(
                "Unknown session handle",
                "Use a live handle returned by frida_attach()",
            )
        })
}

#[cfg(feature = "frida")]
fn script_ptr(handle: i64) -> Result<*mut FridaScript, LegibleError> {
    let index = handle_index(handle, "script")?;
    let (mutex, _) = scripts();
    let guard = mutex.lock().map_err(|_| {
        frida_error(
            "Frida script registry is unavailable",
            "Try again in a new process",
        )
    })?;
    guard
        .get(index)
        .and_then(Option::as_ref)
        .map(|script| script.ptr)
        .ok_or_else(|| {
            frida_error(
                "Unknown script handle",
                "Use a live handle returned by frida_create_script()",
            )
        })
}

#[cfg(feature = "frida")]
fn store_device(ptr: *mut FridaDevice) -> Result<Value, LegibleError> {
    let mut guard = devices().lock().map_err(|_| {
        frida_error(
            "Frida device registry is unavailable",
            "Try again in a new process",
        )
    })?;
    guard.push(Some(DeviceHandle { ptr }));
    Ok(Value::Integer(guard.len() as i64))
}

#[cfg(feature = "frida")]
fn store_session(ptr: *mut FridaSession) -> Result<Value, LegibleError> {
    let mut guard = sessions().lock().map_err(|_| {
        frida_error(
            "Frida session registry is unavailable",
            "Try again in a new process",
        )
    })?;
    guard.push(Some(SessionHandle { ptr }));
    Ok(Value::Integer(guard.len() as i64))
}

#[cfg(feature = "frida")]
fn store_script(ptr: *mut FridaScript) -> Result<i64, LegibleError> {
    let (mutex, _) = scripts();
    let mut guard = mutex.lock().map_err(|_| {
        frida_error(
            "Frida script registry is unavailable",
            "Try again in a new process",
        )
    })?;
    guard.push(Some(ScriptEntry {
        ptr,
        messages: VecDeque::new(),
    }));
    Ok(guard.len() as i64)
}

#[cfg(feature = "frida")]
unsafe fn enumerate_devices() -> Result<Vec<*mut FridaDevice>, LegibleError> {
    let mut error = std::ptr::null_mut();
    let list =
        frida_device_manager_enumerate_devices_sync(manager()?, std::ptr::null_mut(), &mut error);
    ffi_error("Failed to enumerate Frida devices", error)?;
    if list.is_null() {
        return Err(frida_error(
            "Frida returned no device list",
            "Check the Frida installation",
        ));
    }
    let mut result = Vec::new();
    for index in 0..frida_device_list_size(list) {
        let device = frida_device_list_get(list, index);
        if !device.is_null() {
            // `frida_device_list_get` returns an owned device reference. Keep it
            // while releasing the list below, just as Frida's own Rust wrapper does.
            result.push(device);
        }
    }
    frida_unref(list as gpointer);
    Ok(result)
}

#[cfg(feature = "frida")]
unsafe fn device_id(ptr: *mut FridaDevice) -> String {
    CStr::from_ptr(frida_device_get_id(ptr))
        .to_string_lossy()
        .into_owned()
}

#[cfg(feature = "frida")]
unsafe fn device_name(ptr: *mut FridaDevice) -> String {
    CStr::from_ptr(frida_device_get_name(ptr))
        .to_string_lossy()
        .into_owned()
}

#[cfg(feature = "frida")]
fn find_device_by_id(id: &str) -> Result<*mut FridaDevice, LegibleError> {
    let devices = unsafe { enumerate_devices()? };
    for device in devices {
        if unsafe { device_id(device) } == id {
            return Ok(device);
        }
        unsafe { frida_unref(device as gpointer) };
    }
    Err(frida_error(
        &format!("No Frida device with id '{id}'"),
        "Call frida_device_ids() to list available devices",
    ))
}

#[cfg(feature = "frida")]
fn pid_to_guint(pid: i64, name: &str) -> Result<guint, LegibleError> {
    guint::try_from(pid).map_err(|_| {
        frida_error(
            &format!("{name}() expects a non-negative 32-bit process id"),
            "Pass a PID returned by frida_spawn() or frida_device_process_pid()",
        )
    })
}

#[cfg(feature = "frida")]
unsafe extern "C" fn on_message(
    _script: *mut FridaScript,
    message: *const c_char,
    _data: *mut _GBytes,
    user_data: gpointer,
) {
    if message.is_null() || user_data.is_null() {
        return;
    }
    let handle = user_data as usize;
    if handle == 0 {
        return;
    }
    let message = CStr::from_ptr(message).to_string_lossy().into_owned();
    let (mutex, wake) = scripts();
    if let Ok(mut guard) = mutex.lock() {
        if let Some(Some(script)) = guard.get_mut(handle - 1) {
            script.messages.push_back(message);
            wake.notify_all();
        }
    }
}

fn builtin_frida_version(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_version", 0)?;
    #[cfg(feature = "frida")]
    {
        ensure_frida();
        let version = unsafe { frida_version_string() };
        if version.is_null() {
            return Err(frida_error(
                "Frida returned no version string",
                "Check the Frida installation",
            ));
        }
        return Ok(Value::Text(unsafe {
            CStr::from_ptr(version).to_string_lossy().into_owned()
        }));
    }
    #[cfg(not(feature = "frida"))]
    Err(disabled_error())
}

fn builtin_frida_device_ids(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_device_ids", 0)?;
    #[cfg(feature = "frida")]
    {
        let devices = unsafe { enumerate_devices()? };
        let ids = devices
            .into_iter()
            .map(|device| {
                let id = unsafe { device_id(device) };
                unsafe { frida_unref(device as gpointer) };
                Value::Text(id)
            })
            .collect();
        return Ok(Value::list(ids));
    }
    #[cfg(not(feature = "frida"))]
    Err(disabled_error())
}

fn builtin_frida_device_name(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_device_name", 1)?;
    let id = expect_text(args, 0, "frida_device_name")?;
    #[cfg(feature = "frida")]
    {
        let device = find_device_by_id(id)?;
        let name = unsafe { device_name(device) };
        unsafe { frida_unref(device as gpointer) };
        return Ok(Value::Text(name));
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = id;
        Err(disabled_error())
    }
}

fn builtin_frida_open_device(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_open_device", 1)?;
    let id = expect_text(args, 0, "frida_open_device")?;
    #[cfg(feature = "frida")]
    return store_device(find_device_by_id(id)?);
    #[cfg(not(feature = "frida"))]
    {
        let _ = id;
        Err(disabled_error())
    }
}

fn builtin_frida_usb_device(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_usb_device", 1)?;
    let seconds = expect_integer(args, 0, "frida_usb_device")?;
    #[cfg(feature = "frida")]
    {
        if seconds < 0 {
            return Err(frida_error(
                "frida_usb_device() timeout must be non-negative",
                "Pass a non-negative timeout in seconds",
            ));
        }
        let timeout = Duration::from_secs(seconds as u64);
        let started = Instant::now();
        loop {
            for device in unsafe { enumerate_devices()? } {
                if unsafe { frida_device_get_dtype(device) } == 2 {
                    return store_device(device);
                }
                unsafe { frida_unref(device as gpointer) };
            }
            if started.elapsed() >= timeout {
                return Err(frida_error(
                    &format!("No USB Frida device appeared within {seconds} seconds"),
                    "Connect a USB device running Frida and try again",
                ));
            }
            std::thread::sleep(
                Duration::from_millis(100).min(timeout.saturating_sub(started.elapsed())),
            );
        }
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = seconds;
        Err(disabled_error())
    }
}

#[cfg(feature = "frida")]
fn process_entries(device: *mut FridaDevice) -> Result<Vec<(String, i64)>, LegibleError> {
    unsafe {
        let mut error = std::ptr::null_mut();
        let list = frida_device_enumerate_processes_sync(
            device,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut error,
        );
        ffi_error("Failed to enumerate Frida processes", error)?;
        if list.is_null() {
            return Err(frida_error(
                "Frida returned no process list",
                "Check the target device connection",
            ));
        }
        let mut processes = Vec::new();
        for index in 0..frida_process_list_size(list) {
            let process = frida_process_list_get(list, index);
            if !process.is_null() {
                processes.push((
                    CStr::from_ptr(frida_process_get_name(process))
                        .to_string_lossy()
                        .into_owned(),
                    frida_process_get_pid(process) as i64,
                ));
            }
        }
        frida_unref(list as gpointer);
        Ok(processes)
    }
}

fn builtin_frida_device_process_names(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_device_process_names", 1)?;
    let handle = expect_integer(args, 0, "frida_device_process_names")?;
    #[cfg(feature = "frida")]
    return Ok(Value::list(
        process_entries(device_ptr(handle)?)?
            .into_iter()
            .map(|(name, _)| Value::Text(name))
            .collect(),
    ));
    #[cfg(not(feature = "frida"))]
    {
        let _ = handle;
        Err(disabled_error())
    }
}

fn builtin_frida_device_process_pid(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_device_process_pid", 2)?;
    let handle = expect_integer(args, 0, "frida_device_process_pid")?;
    let name = expect_text(args, 1, "frida_device_process_pid")?;
    #[cfg(feature = "frida")]
    {
        let processes = process_entries(device_ptr(handle)?)?;
        if let Some((_, pid)) = processes
            .iter()
            .find(|(process_name, _)| process_name == name)
        {
            return Ok(Value::Integer(*pid));
        }
        let matches: Vec<i64> = processes
            .iter()
            .filter(|(process_name, _)| process_name.contains(name))
            .map(|(_, pid)| *pid)
            .collect();
        return Ok(Value::Integer(if matches.len() == 1 {
            matches[0]
        } else {
            -1
        }));
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = (handle, name);
        Err(disabled_error())
    }
}

fn builtin_frida_spawn(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_spawn", 2)?;
    let handle = expect_integer(args, 0, "frida_spawn")?;
    let program = expect_text(args, 1, "frida_spawn")?;
    #[cfg(feature = "frida")]
    unsafe {
        let program = c_string(program, "frida_spawn")?;
        let options = frida_spawn_options_new();
        if options.is_null() {
            return Err(frida_error(
                "Failed to create Frida spawn options",
                "Check the Frida installation",
            ));
        }
        let mut error = std::ptr::null_mut();
        let pid = frida_device_spawn_sync(
            device_ptr(handle)?,
            program.as_ptr(),
            options,
            std::ptr::null_mut(),
            &mut error,
        );
        frida_unref(options as gpointer);
        ffi_error("Failed to spawn process", error)?;
        return Ok(Value::Integer(pid as i64));
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = (handle, program);
        Err(disabled_error())
    }
}

fn builtin_frida_resume(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_resume", 2)?;
    let handle = expect_integer(args, 0, "frida_resume")?;
    let pid = expect_integer(args, 1, "frida_resume")?;
    #[cfg(feature = "frida")]
    unsafe {
        let mut error = std::ptr::null_mut();
        frida_device_resume_sync(
            device_ptr(handle)?,
            pid_to_guint(pid, "frida_resume")?,
            std::ptr::null_mut(),
            &mut error,
        );
        ffi_error("Failed to resume process", error)?;
        return Ok(Value::None);
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = (handle, pid);
        Err(disabled_error())
    }
}

fn builtin_frida_kill(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_kill", 2)?;
    let handle = expect_integer(args, 0, "frida_kill")?;
    let pid = expect_integer(args, 1, "frida_kill")?;
    #[cfg(feature = "frida")]
    unsafe {
        let mut error = std::ptr::null_mut();
        frida_device_kill_sync(
            device_ptr(handle)?,
            pid_to_guint(pid, "frida_kill")?,
            std::ptr::null_mut(),
            &mut error,
        );
        if let Some(message) = take_gerror(error) {
            let gone = [
                "not found",
                "no such process",
                "not running",
                "already exited",
                "process is gone",
                "unable to find process",
            ]
            .iter()
            .any(|needle| message.to_ascii_lowercase().contains(needle));
            if !gone {
                return Err(frida_error(
                    &format!("Failed to kill process: {message}"),
                    "Check the device and process id",
                ));
            }
        }
        return Ok(Value::None);
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = (handle, pid);
        Err(disabled_error())
    }
}

fn builtin_frida_attach(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_attach", 2)?;
    let handle = expect_integer(args, 0, "frida_attach")?;
    let pid = expect_integer(args, 1, "frida_attach")?;
    #[cfg(feature = "frida")]
    unsafe {
        let options = frida_session_options_new();
        if options.is_null() {
            return Err(frida_error(
                "Failed to create Frida session options",
                "Check the Frida installation",
            ));
        }
        let mut error = std::ptr::null_mut();
        let session = frida_device_attach_sync(
            device_ptr(handle)?,
            pid_to_guint(pid, "frida_attach")?,
            options,
            std::ptr::null_mut(),
            &mut error,
        );
        frida_unref(options as gpointer);
        ffi_error("Failed to attach to process", error)?;
        if session.is_null() {
            return Err(frida_error(
                "Frida returned no session",
                "Check the device and process id",
            ));
        }
        return store_session(session);
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = (handle, pid);
        Err(disabled_error())
    }
}

fn builtin_frida_detach(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_detach", 1)?;
    let handle = expect_integer(args, 0, "frida_detach")?;
    #[cfg(feature = "frida")]
    unsafe {
        let mut error = std::ptr::null_mut();
        frida_session_detach_sync(session_ptr(handle)?, std::ptr::null_mut(), &mut error);
        ffi_error("Failed to detach Frida session", error)?;
        return Ok(Value::None);
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = handle;
        Err(disabled_error())
    }
}

fn builtin_frida_create_script(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_create_script", 2)?;
    let session = expect_integer(args, 0, "frida_create_script")?;
    let source = expect_text(args, 1, "frida_create_script")?;
    #[cfg(feature = "frida")]
    unsafe {
        let source = c_string(source, "frida_create_script")?;
        let options = frida_script_options_new();
        if options.is_null() {
            return Err(frida_error(
                "Failed to create Frida script options",
                "Check the Frida installation",
            ));
        }
        let mut error = std::ptr::null_mut();
        let script = frida_session_create_script_sync(
            session_ptr(session)?,
            source.as_ptr(),
            options,
            std::ptr::null_mut(),
            &mut error,
        );
        frida_unref(options as gpointer);
        ffi_error("Failed to create Frida script", error)?;
        if script.is_null() {
            return Err(frida_error(
                "Frida returned no script",
                "Check the script source",
            ));
        }
        let handle = store_script(script)?;
        let signal = c_string("message", "frida_create_script")?;
        let callback: GCallback = Some(std::mem::transmute::<*mut c_void, unsafe extern "C" fn()>(
            on_message as *mut c_void,
        ));
        let signal_id = _frida_g_signal_connect_data(
            script as gpointer,
            signal.as_ptr(),
            callback,
            handle as gpointer,
            None,
            0,
        );
        if signal_id == 0 {
            return Err(frida_error(
                "Failed to connect Frida script message handler",
                "Check the Frida installation",
            ));
        }
        return Ok(Value::Integer(handle));
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = (session, source);
        Err(disabled_error())
    }
}

fn builtin_frida_load_script(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_load_script", 1)?;
    let handle = expect_integer(args, 0, "frida_load_script")?;
    #[cfg(feature = "frida")]
    unsafe {
        let mut error = std::ptr::null_mut();
        frida_script_load_sync(script_ptr(handle)?, std::ptr::null_mut(), &mut error);
        ffi_error("Failed to load Frida script", error)?;
        return Ok(Value::None);
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = handle;
        Err(disabled_error())
    }
}

fn builtin_frida_unload_script(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_unload_script", 1)?;
    let handle = expect_integer(args, 0, "frida_unload_script")?;
    #[cfg(feature = "frida")]
    unsafe {
        let mut error = std::ptr::null_mut();
        frida_script_unload_sync(script_ptr(handle)?, std::ptr::null_mut(), &mut error);
        ffi_error("Failed to unload Frida script", error)?;
        return Ok(Value::None);
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = handle;
        Err(disabled_error())
    }
}

#[cfg(feature = "frida")]
fn pop_message(handle: i64) -> Result<Option<String>, LegibleError> {
    let index = handle_index(handle, "script")?;
    let (mutex, _) = scripts();
    let mut guard = mutex.lock().map_err(|_| {
        frida_error(
            "Frida script registry is unavailable",
            "Try again in a new process",
        )
    })?;
    guard
        .get_mut(index)
        .and_then(Option::as_mut)
        .map(|script| script.messages.pop_front())
        .ok_or_else(|| {
            frida_error(
                "Unknown script handle",
                "Use a live handle returned by frida_create_script()",
            )
        })
}

fn builtin_frida_next_message(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_next_message", 1)?;
    let handle = expect_integer(args, 0, "frida_next_message")?;
    #[cfg(feature = "frida")]
    return Ok(Value::Text(pop_message(handle)?.unwrap_or_default()));
    #[cfg(not(feature = "frida"))]
    {
        let _ = handle;
        Err(disabled_error())
    }
}

fn builtin_frida_wait_message(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "frida_wait_message", 2)?;
    let handle = expect_integer(args, 0, "frida_wait_message")?;
    let timeout_ms = expect_integer(args, 1, "frida_wait_message")?;
    #[cfg(feature = "frida")]
    {
        if timeout_ms < 0 {
            return Err(frida_error(
                "frida_wait_message() timeout must be non-negative",
                "Pass a non-negative timeout in milliseconds",
            ));
        }
        let index = handle_index(handle, "script")?;
        let (mutex, wake) = scripts();
        let mut guard = mutex.lock().map_err(|_| {
            frida_error(
                "Frida script registry is unavailable",
                "Try again in a new process",
            )
        })?;
        let deadline = Duration::from_millis(timeout_ms as u64);
        let started = Instant::now();
        loop {
            let script = guard
                .get_mut(index)
                .and_then(Option::as_mut)
                .ok_or_else(|| {
                    frida_error(
                        "Unknown script handle",
                        "Use a live handle returned by frida_create_script()",
                    )
                })?;
            if let Some(message) = script.messages.pop_front() {
                return Ok(Value::Text(message));
            }
            let remaining = match deadline.checked_sub(started.elapsed()) {
                Some(remaining) => remaining,
                None => return Ok(Value::Text(String::new())),
            };
            let (next_guard, result) = wake.wait_timeout(guard, remaining).map_err(|_| {
                frida_error(
                    "Frida message queue is unavailable",
                    "Try again in a new process",
                )
            })?;
            guard = next_guard;
            if result.timed_out() {
                return Ok(Value::Text(String::new()));
            }
        }
    }
    #[cfg(not(feature = "frida"))]
    {
        let _ = (handle, timeout_ms);
        Err(disabled_error())
    }
}
