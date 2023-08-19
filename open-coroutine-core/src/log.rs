#[cfg(feature = "logs")]
pub static LOG: std::sync::Once = std::sync::Once::new();

#[macro_export]
macro_rules! info {
    // info!(target: "my_target", key1 = 42, key2 = true; "a {} event", "log")
    // info!(target: "my_target", "a {} event", "log")
    (target: $target:expr, $($arg:tt)+) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "logs")] {
                $crate::log::LOG.call_once(|| {
                    _ = simplelog::CombinedLogger::init(vec![simplelog::TermLogger::new(
                        log::LevelFilter::Info,
                        simplelog::ConfigBuilder::new()
                            .set_time_format_rfc2822()
                            .set_time_offset_to_local()
                            .map(simplelog::ConfigBuilder::build)
                            .map_err(simplelog::ConfigBuilder::build)
                            .unwrap(),
                        simplelog::TerminalMode::Mixed,
                        simplelog::ColorChoice::Auto,
                    )]);
                });
                log::log!(target: $target, log::Level::Info, $($arg)+)
            }
        }
    };

    // info!("a {} event", "log")
    ($($arg:tt)+) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "logs")] {
                $crate::log::LOG.call_once(|| {
                    _ = simplelog::CombinedLogger::init(vec![simplelog::TermLogger::new(
                        log::LevelFilter::Info,
                        simplelog::ConfigBuilder::new()
                            .set_time_format_rfc2822()
                            .set_time_offset_to_local()
                            .map(simplelog::ConfigBuilder::build)
                            .map_err(simplelog::ConfigBuilder::build)
                            .unwrap(),
                        simplelog::TerminalMode::Mixed,
                        simplelog::ColorChoice::Auto,
                    )]);
                });
                log::log!(log::Level::Info, $($arg)+)
            }
        }
    }
}
