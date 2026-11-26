use infrastructure::logging::{LogConfig, LogGuard, LogRuntime};
use tracing_subscriber::prelude::*;

pub(crate) fn initialize() -> (LogRuntime, LogGuard) {
    let directory = dirs::data_local_dir()
        .expect("local application data directory")
        .join("Meow_Detective/logs");
    let (runtime, guard) = LogRuntime::start(LogConfig::new(directory)).unwrap_or_else(|error| {
        show_failure(&error.to_string());
        std::process::exit(1);
    });
    tracing_subscriber::registry()
        .with(
            runtime
                .layer()
                .with_filter(tracing_subscriber::filter::LevelFilter::INFO),
        )
        .try_init()
        .expect("application tracing subscriber");
    let panic_logs = runtime.clone();
    std::panic::set_hook(Box::new(move |panic| {
        let location = panic
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()));
        let message = panic
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("application panic");
        tracing::error!(location, panic_message = message, "Application thread panicked");
        if let Err(error) = panic_logs.flush() {
            show_failure(&error.to_string());
        }
    }));
    (runtime, guard)
}

pub(crate) fn show_failure(message: &str) {
    #[cfg(windows)]
    {
        let title: Vec<u16> = "Meow~Detective".encode_utf16().chain(Some(0)).collect();
        let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        // SAFETY: both UTF-16 buffers are terminated and remain valid for this synchronous call.
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                windows_sys::Win32::UI::WindowsAndMessaging::MB_OK
                    | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}
