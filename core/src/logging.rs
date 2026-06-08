use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the tracing subscriber.
///
/// - **debug build** — stdout only
/// - **release build** — rolling daily file in the OS data directory
/// - **`verbose = true`** — file + stdout regardless of build profile
pub fn init(verbose: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(LevelFilter::INFO.to_string()));

    let stdout_layer = fmt::layer().with_target(false).with_filter(filter.clone());

    #[cfg(debug_assertions)]
    {
        let _ = verbose;
        tracing_subscriber::registry().with(stdout_layer).init();
    }

    #[cfg(not(debug_assertions))]
    {
        use crate::{APP_DATA_DIR, APP_NAME};
        use tracing_appender::rolling;

        std::fs::create_dir_all(APP_DATA_DIR.as_path()).ok();

        let file_appender = rolling::daily(
            APP_DATA_DIR.as_path(),
            format!("{}.log", APP_NAME.to_lowercase()),
        );
        let file_layer = fmt::layer()
            .with_ansi(false)
            .with_target(false)
            .with_writer(file_appender)
            .with_filter(filter);

        if verbose {
            tracing_subscriber::registry()
                .with(file_layer)
                .with(stdout_layer)
                .init();
        } else {
            tracing_subscriber::registry().with(file_layer).init();
        }
    }
}

pub fn init_stdout() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(LevelFilter::INFO.to_string()));

    let stdout_layer = fmt::layer().with_target(false).with_filter(filter.clone());

    tracing_subscriber::registry().with(stdout_layer).init();
}
