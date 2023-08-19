use open_coroutine_core::event_loop::EventLoops;
use retour::static_detour;
use std::error::Error;
use std::os::raw::c_void;
use std::time::Duration;
use std::{ffi::CString, iter, mem};
use windows_sys::Win32::Foundation::BOOL;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

#[no_mangle]
#[allow(non_snake_case, warnings)]
pub unsafe extern "system" fn DllMain(
    _module: *mut c_void,
    call_reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    if call_reason == DLL_PROCESS_ATTACH {
        // A console may be useful for printing to 'stdout'
        // winapi::um::consoleapi::AllocConsole();

        // Preferably a thread should be created here instead, since as few
        // operations as possible should be performed within `DllMain`.
        main().is_ok() as BOOL
    } else {
        1
    }
}

static_detour! {
  static SleepHook: unsafe extern "system" fn(u32);
}

// A type alias for `FnSleep` (makes the transmute easy on the eyes)
type FnSleep = unsafe extern "system" fn(u32);

/// Called when the DLL is attached to the process.
unsafe fn main() -> Result<(), Box<dyn Error>> {
    // Retrieve an absolute address of `MessageBoxW`. This is required for
    // libraries due to the import address table. If `MessageBoxW` would be
    // provided directly as the target, it would only hook this DLL's
    // `MessageBoxW`. Using the method below an absolute address is retrieved
    // instead, detouring all invocations of `MessageBoxW` in the active process.
    let address =
        get_module_symbol_address("kernel32.dll", "Sleep").expect("could not find 'Sleep' address");
    let target: FnSleep = mem::transmute(address);

    // Initialize AND enable the detour (the 2nd parameter can also be a closure)
    SleepHook.initialize(target, sleep_detour)?.enable()?;
    Ok(())
}

/// Returns a module symbol's absolute address.
fn get_module_symbol_address(module: &str, symbol: &str) -> Option<usize> {
    let module = module
        .encode_utf16()
        .chain(iter::once(0))
        .collect::<Vec<u16>>();
    let symbol = CString::new(symbol).unwrap();
    unsafe {
        let handle = GetModuleHandleW(module.as_ptr());
        GetProcAddress(handle, symbol.as_ptr() as _).map(|n| n as usize)
    }
}

fn sleep_detour(dw_milliseconds: u32) {
    open_coroutine_core::info!("Sleep hooked");
    _ = EventLoops::wait_event(Some(Duration::from_millis(u64::from(dw_milliseconds))));
}
