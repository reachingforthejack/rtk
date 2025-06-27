use std::{fmt::Display, path::PathBuf};

use mlua::FromLua;
use rtk_lua_macros::RtkMeta;

/// The rustc driver version this script is requesting. This enum MUST remain stable. Do not change
/// or remove any existing, committed values to this. You can only add more.
#[derive(Clone, Eq, PartialEq, Debug, RtkMeta)]
pub enum RtkRustcDriverVersion {
    /// The latest version on crates io. Probably not a great idea to use this!
    #[rtk_meta(override = string)]
    CratesIoLatest,

    /// Specific version on crates io.
    #[rtk_meta(override = string)]
    CratesIo { major: u32, minor: u32, patch: u32 },

    /// A local version of the driver.
    #[rtk_meta(override = string)]
    Local { path: PathBuf },
}

impl FromLua for RtkRustcDriverVersion {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        let value_str = value.to_string()?;
        match value_str.as_str() {
            "latest" => Ok(RtkRustcDriverVersion::CratesIoLatest),
            local if local.starts_with("local:") => {
                let path_str = local.trim_start_matches("local:");
                let path = PathBuf::from(path_str);
                Ok(RtkRustcDriverVersion::Local { path })
            }
            crates_io => {
                let parts: Vec<&str> = crates_io.split('.').collect();
                if parts.len() != 3 {
                    return Err(mlua::Error::external(format!(
                        "Invalid version format: {crates_io}. Expected format: major.minor.patch",
                    )));
                }

                let major = parts[0].parse::<u32>().map_err(|_| {
                    mlua::Error::external(format!("Invalid major version: {}", parts[0]))
                })?;

                let minor = parts[1].parse::<u32>().map_err(|_| {
                    mlua::Error::external(format!("Invalid minor version: {}", parts[1]))
                })?;

                let patch = parts[2].parse::<u32>().map_err(|_| {
                    mlua::Error::external(format!("Invalid patch version: {}", parts[2]))
                })?;

                Ok(RtkRustcDriverVersion::CratesIo {
                    major,
                    minor,
                    patch,
                })
            }
        }
    }
}

impl Display for RtkRustcDriverVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RtkRustcDriverVersion::CratesIoLatest => write!(f, "latest"),
            RtkRustcDriverVersion::CratesIo {
                major,
                minor,
                patch,
            } => {
                write!(f, "{major}.{minor}.{patch}")
            }
            RtkRustcDriverVersion::Local { path } => write!(f, "local:{}", path.display()),
        }
    }
}
