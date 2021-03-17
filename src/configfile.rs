use config::Config;
use lazy_static;
use std::io;
use std::io::BufRead;
use std::sync::RwLock;

lazy_static::lazy_static! {
    pub static ref SETTINGS: RwLock<Config> = RwLock::new(Config::default());
}

pub(crate) fn get_settings(mut configfile: impl BufRead) -> io::Result<()> {
    // let mut settings = Config::default();
    let mut config_content = String::new();
    configfile.read_to_string(&mut config_content)?;
    let config = config::File::from_str(&config_content, config::FileFormat::Toml);
    if let Err(err) = SETTINGS.write().unwrap().merge(config) {
        println!("Cannot read config file, ignoring: {:?}", err);
    }
    Ok(())
}
