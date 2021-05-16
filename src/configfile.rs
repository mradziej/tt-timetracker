use crate::error::TTError;
use config::{Config, ConfigError};
use std::cell::RefCell;
use std::io::BufRead;
use std::rc::Rc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct WatchI3Config {
    pub granularity: Duration,
    pub timeblock: Duration,
    pub timeblock_empty: Duration,
}

#[derive(Debug, Clone)]
pub struct TTConfig {
    pub prefix: Option<String>,
    pub watch_i3: WatchI3Config,
}

const DEFAULT: TTConfig = TTConfig {
    prefix: None,
    watch_i3: WatchI3Config {
        granularity: Duration::from_secs(10),
        timeblock: Duration::from_secs(120),
        timeblock_empty: Duration::from_secs(300),
    },
};

thread_local! {
    static SETTINGS: RefCell<Rc<TTConfig>> = RefCell::new(Rc::new(TTConfig::default()));
}

fn or_none<T>(value: Result<T, ConfigError>) -> Result<Option<T>, ConfigError> {
    match value {
        Ok(value) => Ok(Some(value)),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(other) => Err(other),
    }
}

impl TTConfig {
    pub fn init(mut configfile: impl BufRead) -> Result<(), TTError> {
        let mut config_content = String::new();
        configfile.read_to_string(&mut config_content)?;
        let mut default = Config::default();
        let config = default.merge(config::File::from_str(
            &config_content,
            config::FileFormat::Toml,
        ))?;
        let new_config = TTConfig {
            prefix: or_none(config.get_str("prefix"))?,
            watch_i3: WatchI3Config {
                granularity: Duration::from_secs(
                    config.get_int("watch-i3.granularity").unwrap_or(10) as u64,
                ),
                timeblock: Duration::from_secs(
                    config.get_int("watch-i3.timeblock").unwrap_or(120) as u64
                ),
                timeblock_empty: Duration::from_secs(
                    config.get_int("watch-i3.timeblock_empty").unwrap_or(600) as u64,
                ),
            },
        };
        SETTINGS.with(move |settings| {
            settings.replace(Rc::new(new_config));
        });
        Ok(())
    }

    pub fn get() -> Rc<TTConfig> {
        SETTINGS.with(|settings| settings.borrow().clone())
    }
}

impl Default for TTConfig {
    fn default() -> Self {
        DEFAULT.clone()
    }
}
