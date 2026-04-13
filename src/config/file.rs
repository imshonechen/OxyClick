use std::fs;
use std::path::{Path, PathBuf};

use crate::config::schema::AppConfig;
use crate::error::AppError;

pub fn default_config_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("OxyClick")
        .join("config.toml")
}

pub fn portable_config_path() -> PathBuf {
    PathBuf::from(".").join("config").join("config.toml")
}

pub fn write_config(path: &Path, config: &AppConfig) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, config.to_toml_string()?)?;
    Ok(())
}

pub fn load_or_default(path: &Path) -> Result<AppConfig, AppError> {
    if path.exists() {
        let contents = fs::read_to_string(path)?;
        return AppConfig::from_toml_str(&contents);
    }

    let config = AppConfig::default();
    write_config(path, &config)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{load_or_default, write_config};
    use crate::config::schema::AppConfig;

    fn unique_temp_path() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();

        std::env::temp_dir()
            .join("oxyclick-tests")
            .join(format!("config-{suffix}.toml"))
    }

    #[test]
    fn creates_default_config_when_missing() {
        let path = unique_temp_path();
        let config = load_or_default(&path).expect("missing config should be created");

        assert_eq!(config, AppConfig::default());
        assert!(path.exists());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn writes_and_reads_round_trip() {
        let path = unique_temp_path();
        let mut config = AppConfig::default();
        config.active_profile_mut().name = String::from("Saved Profile");

        write_config(&path, &config).expect("config should write");
        let loaded = load_or_default(&path).expect("config should load");

        assert_eq!(loaded, config);

        let _ = std::fs::remove_file(path);
    }
}
