#[cfg(feature = "logs")]
pub static LOG: std::sync::Once = std::sync::Once::new();

#[macro_export]
macro_rules! init_log {
    () => {
        $crate::log::LOG.call_once(|| {
            let mut builder = simplelog::ConfigBuilder::new();
            let result = builder.set_time_format_rfc2822().set_time_offset_to_local();
            let config = if let Ok(builder) = result {
                builder
            } else {
                result.unwrap_err()
            }
            .build();
            _ = simplelog::CombinedLogger::init(vec![simplelog::TermLogger::new(
                log::LevelFilter::Info,
                config,
                simplelog::TerminalMode::Mixed,
                simplelog::ColorChoice::Auto,
            )]);
        });
    };
}

#[macro_export]
macro_rules! info {
    // info!(target: "my_target", key1 = 42, key2 = true; "a {} event", "log")
    // info!(target: "my_target", "a {} event", "log")
    (target: $target:expr, $($arg:tt)+) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "logs")] {
                $crate::init_log!();
                log::log!(target: $target, log::Level::Info, $($arg)+)
            }
        }
    };

    // info!("a {} event", "log")
    ($($arg:tt)+) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "logs")] {
                $crate::init_log!();
                log::log!(log::Level::Info, $($arg)+)
            }
        }
    }
}

#[macro_export]
macro_rules! warn {
    // warn!(target: "my_target", key1 = 42, key2 = true; "a {} event", "log")
    // warn!(target: "my_target", "a {} event", "log")
    (target: $target:expr, $($arg:tt)+) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "logs")] {
                $crate::init_log!();
                log::log!(target: $target, log::Level::Warn, $($arg)+)
            }
        }
    };

    // warn!("a {} event", "log")
    ($($arg:tt)+) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "logs")] {
                $crate::init_log!();
                log::log!(log::Level::Warn, $($arg)+)
            }
        }
    }
}

#[macro_export]
macro_rules! error {
    // error!(target: "my_target", key1 = 42, key2 = true; "a {} event", "log")
    // error!(target: "my_target", "a {} event", "log")
    (target: $target:expr, $($arg:tt)+) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "logs")] {
                $crate::init_log!();
                log::log!(target: $target, log::Level::Error, $($arg)+)
            }
        }

    };

    // error!("a {} event", "log")
    ($($arg:tt)+) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "logs")] {
                $crate::init_log!();
                log::log!(log::Level::Error, $($arg)+)
            }
        }

    }
}
