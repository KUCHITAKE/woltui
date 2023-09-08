use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io,
    path::Path,
};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Machine {
    pub name: String,
    pub mac_address: String,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub machines: Vec<Machine>,
}

pub fn read_config(file_path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    if !file_path.exists() {
        let _ = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(file_path)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        return Ok(Config::default());
    }

    let content =
        fs::read_to_string(file_path).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let config = toml::from_str(&content)?;

    Ok(config)
}

pub fn write_config(file_path: &Path, config: &Config) -> io::Result<()> {
    if let Some(parent_dir) = file_path.parent() {
        if !parent_dir.exists() {
            fs::create_dir_all(parent_dir)?
        }
    }

    let content =
        toml::to_string(config).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(file_path, content)
}
