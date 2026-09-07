use std::{error::Error, fs, io, path::Path};

static ARCHIVE: &[u8] = include_bytes!(env!("XMRIG_ARCHIVE"));
const CUSTOM_CONFIG: &str = include_str!("../xmrig/config.json");

#[cfg(target_os = "windows")]
const RUN_FILE: Option<(&str, &[u8])> =
    Some(("windows.bat", include_bytes!("../xmrig/windows.bat")));
#[cfg(not(target_os = "windows"))]
const RUN_FILE: Option<(&str, &[u8])> = None;

fn install_run_file(destination: &Path) -> io::Result<()> {
    if let Some((name, contents)) = RUN_FILE {
        let path = destination.join(name);
        fs::write(&path, contents)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

fn custom_config(username: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut config: serde_json::Value = serde_json::from_str(CUSTOM_CONFIG)?;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        config["pause-on-active"] = true.into();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    config
        .as_object_mut()
        .ok_or("Config must be an object")?
        .remove("pause-on-active");
    #[cfg(target_os = "macos")]
    {
        let settings = config.as_object_mut().ok_or("Config must be an object")?;
        settings.remove("cuda");
    }
    let pools = config["pools"]
        .as_array_mut()
        .ok_or("Config pools must be an array")?;
    for pool in pools {
        if pool["pass"].as_str() == Some("anon") {
            pool["pass"] = username.into();
        }
    }
    Ok(serde_json::to_vec_pretty(&config)?)
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(io::Error::other(format!(
            "Refusing non-directory {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(path),
        Err(error) => Err(error),
    }
}

// Install into the freshly cleared destination without following existing links.
fn install_directory(source: &Path, destination: &Path) -> io::Result<()> {
    ensure_directory(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            install_directory(&entry.path(), &target)?;
        } else if kind.is_file() {
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            io::copy(&mut fs::File::open(entry.path())?, &mut output)?;
            output.set_permissions(entry.metadata()?.permissions())?;
        } else {
            return Err(io::Error::other("Archive contains a link or special file"));
        }
    }
    Ok(())
}

fn extract(destination: &Path) -> Result<(), Box<dyn Error>> {
    let config = custom_config(&whoami::fallible::username()?)?;
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(destination)?,
        Ok(_) => return Err(format!("Refusing non-directory {}", destination.display()).into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let staging = tempfile::tempdir()?;
    #[cfg(windows)]
    zip::ZipArchive::new(std::io::Cursor::new(ARCHIVE))?.extract(staging.path())?;
    #[cfg(not(windows))]
    tar::Archive::new(flate2::read::GzDecoder::new(ARCHIVE)).unpack(staging.path())?;
    let entries = fs::read_dir(staging.path())?.collect::<Result<Vec<_>, _>>()?;
    if entries.len() != 1 || !entries[0].file_type()?.is_dir() {
        return Err("Expected one top-level XMRig directory in the release archive".into());
    }
    install_directory(&entries[0].path(), destination)?;
    fs::write(destination.join("config.json"), config)?;
    install_run_file(destination)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(windows)]
    let destination = {
        let temp = std::env::var_os("TEMP").ok_or("TEMP is not set")?;
        let temp = Path::new(&temp);
        if !temp.is_absolute() {
            return Err("TEMP must be an absolute path".into());
        }
        ensure_directory(temp)?;
        temp.join("xmrig")
    };
    #[cfg(not(windows))]
    let destination = dirs::home_dir()
        .ok_or("Cannot determine the user's home directory")?
        .join("xmrig");
    extract(&destination)?;
    println!(
        "Extracted fresh XMRig {} to {}",
        env!("XMRIG_VERSION"),
        destination.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_config_replaces_worker_name_and_preserves_other_settings() {
        let username = "test\\user\"name";
        let actual: serde_json::Value =
            serde_json::from_slice(&custom_config(username).unwrap()).unwrap();
        let mut expected: serde_json::Value = serde_json::from_str(CUSTOM_CONFIG).unwrap();
        assert_eq!(expected["pools"][0]["pass"], "anon");
        expected["pools"][0]["pass"] = username.into();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            expected["pause-on-active"] = true.into();
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        expected.as_object_mut().unwrap().remove("pause-on-active");
        #[cfg(target_os = "macos")]
        {
            expected.as_object_mut().unwrap().remove("cuda");
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn extracts_embedded_release_and_replaces_existing_directory() {
        let home = tempfile::tempdir().unwrap();
        let destination = home.path().join("xmrig");
        extract(&destination).unwrap();
        for name in ["windows.bat", "macos.sh", "linux.sh"] {
            let path = destination.join(name);
            if let Some((selected, contents)) = RUN_FILE.filter(|(selected, _)| *selected == name) {
                assert_eq!(fs::read(destination.join(selected)).unwrap(), contents);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(
                        fs::metadata(path).unwrap().permissions().mode() & 0o777,
                        0o755
                    );
                }
            } else {
                assert!(!path.exists(), "Unexpected run file: {name}");
            }
        }
        let executable = destination.join(if cfg!(windows) { "xmrig.exe" } else { "xmrig" });
        assert!(executable.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(&executable).unwrap().permissions().mode() & 0o111,
                0
            );
        }
        let original_config = fs::read(destination.join("config.json")).unwrap();
        assert_eq!(
            original_config,
            custom_config(&whoami::fallible::username().unwrap()).unwrap()
        );
        let original_executable = fs::read(&executable).unwrap();
        fs::write(destination.join("config.json"), "custom configuration").unwrap();
        fs::write(&executable, "old executable").unwrap();
        fs::create_dir(destination.join("old-folder")).unwrap();
        fs::write(destination.join("old-folder/stale.txt"), "stale").unwrap();
        extract(&destination).unwrap();
        assert_eq!(
            fs::read(destination.join("config.json")).unwrap(),
            original_config
        );
        assert_eq!(fs::read(&executable).unwrap(), original_executable);
        assert!(!destination.join("old-folder").exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_destination() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("xmrig");
        std::os::unix::fs::symlink(temp.path(), &destination).unwrap();
        assert!(extract(&destination).is_err());
    }
}
