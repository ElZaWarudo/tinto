use std::path::PathBuf;

/// Resolves Tinto's state directory.
///
/// E2E binaries are deliberately unable to fall back to the user's real
/// configuration directory. The runner must provide an isolated directory.
pub(crate) fn tinto_config_dir() -> Option<PathBuf> {
    #[cfg(feature = "e2e-wdio")]
    {
        std::env::var_os("TINTO_E2E_CONFIG_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    #[cfg(not(feature = "e2e-wdio"))]
    {
        dirs::config_dir().map(|dir| dir.join("tinto"))
    }
}

/// Resolves the user's home directory without allowing E2E binaries to fall
/// back to the real profile on platforms where `dirs::home_dir()` ignores
/// environment overrides.
pub(crate) fn user_home_dir() -> Option<PathBuf> {
    #[cfg(feature = "e2e-wdio")]
    {
        std::env::var_os("TINTO_E2E_HOME_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    #[cfg(not(feature = "e2e-wdio"))]
    {
        dirs::home_dir()
    }
}

pub(crate) fn data_local_dir() -> Option<PathBuf> {
    #[cfg(feature = "e2e-wdio")]
    {
        std::env::var_os("TINTO_E2E_DATA_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    #[cfg(not(feature = "e2e-wdio"))]
    {
        dirs::data_local_dir()
    }
}

#[cfg(feature = "e2e-wdio")]
pub(crate) fn e2e_webview_data_dir() -> Option<PathBuf> {
    std::env::var_os("TINTO_E2E_WEBVIEW_DATA_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
