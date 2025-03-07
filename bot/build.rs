use failure::{format_err, ResultExt as _};
use std::{env, fs, path::PathBuf, process::Command};

type Result<T> = std::result::Result<T, failure::Error>;

fn version() -> Result<String> {
    if let Ok(version) = env::var("OXIDIZE_VERSION") {
        return Ok(version);
    }

    let output = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()?;

    let version = std::str::from_utf8(&output.stdout)?;
    Ok(format!("git-{}", version))
}

/// Construct a Windows version information.
fn file_version() -> Option<(String, u64)> {
    let version = match env::var("OXIDIZE_FILE_VERSION") {
        Ok(version) => version,
        Err(_) => return None,
    };

    let mut info = 0u64;

    let mut it = version.split('.');

    info |= it.next()?.parse().unwrap_or(0) << 48;
    info |= it.next()?.parse().unwrap_or(0) << 32;
    info |= it.next()?.parse().unwrap_or(0) << 16;
    info |= match it.next() {
        Some(n) => n.parse().unwrap_or(0),
        None => 0,
    };

    Some((version, info))
}

fn main() -> Result<()> {
    let version = version()?;

    if cfg!(target_os = "windows") {
        use winres::VersionInfo::*;

        let mut res = winres::WindowsResource::new();
        res.set_icon("res/icon.ico");

        if let Some((version, info)) = file_version() {
            res.set("FileVersion", &version);
            res.set("ProductVersion", &version);
            res.set_version_info(FILEVERSION, info);
            res.set_version_info(PRODUCTVERSION, info);
        }

        res.compile().context("compiling resorces")?;
    }

    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").ok_or_else(|| format_err!("missing: OUT_DIR"))?);
    fs::write(out_dir.join("version.txt"), &version).context("writing version.txt")?;
    Ok(())
}
