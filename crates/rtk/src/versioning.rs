use std::{
    path::PathBuf,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Context;
use rtk_lua::{RtkLuaScriptExecutor, RtkRustcDriverVersion};

use crate::DRIVER_NAME;

/// Extract the desired driver version from the script, returning the target release version and
/// optionally the desired debug version
pub fn desired_version_for_script(
    script: &str,
) -> anyhow::Result<(RtkRustcDriverVersion, Option<RtkRustcDriverVersion>)> {
    let v = PreflightRtkVersioner::default();
    let lua = rtk_lua::RtkLua::new(v.clone()).context("failed to create Lua instance")?;

    // we can deliberately ignore an error here, since its very possible the script execution will
    // fail if the user currently is on a different version of the cli where the `rtk_lua` api is
    // different. we don't actually care about errors, we just need to extract the version so as
    // long as the error occured after the version was set we're fine
    let _ = lua.execute(script);

    if v.version_double_set_attempted.load(Ordering::Relaxed) {
        return Err(anyhow::anyhow!(
            "Lua script attempted to set the desired version multiple times, the desired version should be specified first and once"
        ));
    }

    let release_version = v
        .version
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| anyhow::anyhow!("No version was set in the Lua script"))?;

    let debug_version = v.debug_version.lock().unwrap().take();

    Ok((release_version, debug_version))
}

/// Before running the Lua script against the real rustc driver, we do a dry run of the lua script
/// to check it for errors and to extract the required version of the Rustc driver
#[derive(Clone, Default)]
struct PreflightRtkVersioner {
    version: Arc<Mutex<Option<RtkRustcDriverVersion>>>,
    debug_version: Arc<Mutex<Option<RtkRustcDriverVersion>>>,
    version_double_set_attempted: Arc<AtomicBool>,
}

impl RtkLuaScriptExecutor for PreflightRtkVersioner {
    fn intake_version(&self, version: RtkRustcDriverVersion) {
        let mut curr_version = self.version.lock().unwrap();
        if curr_version.is_some() {
            self.version_double_set_attempted
                .store(true, Ordering::Relaxed);
        } else {
            curr_version.replace(version);
        }
    }

    fn intake_debug_version(&self, version: RtkRustcDriverVersion) {
        let mut curr_version = self.debug_version.lock().unwrap();
        if curr_version.is_some() {
            self.version_double_set_attempted
                .store(true, Ordering::Relaxed);
        }
        curr_version.replace(version);
    }

    fn query_method_calls(&self, _query: rtk_lua::MethodCallQuery) -> Vec<rtk_lua::MethodCall> {
        vec![]
    }

    fn query_functions(&self, _query: rtk_lua::Location) -> Vec<rtk_lua::FunctionTypeValue> {
        vec![]
    }

    fn query_trait_impls(&self, _query: rtk_lua::Location) -> Vec<rtk_lua::TraitImpl> {
        vec![]
    }

    fn query_function_calls(&self, _query: rtk_lua::Location) -> Vec<rtk_lua::FunctionCall> {
        vec![]
    }

    fn log_note(&self, _msg: String) {}

    fn log_warn(&self, _msg: String) {}

    fn log_error(&self, _msg: String) {}

    fn log_fatal_error(&self, msg: String) -> ! {
        panic!("fatal error hit in preflight script check: {msg}")
    }

    fn emit(&self, _text: String) {}
}

pub fn install_rtk_rustc_driver(version: RtkRustcDriverVersion) -> anyhow::Result<()> {
    let currently_installed_version = currently_installed_rtk_rustc_driver_version(
        #[cfg(test)]
        "",
    )
    .context("failed to get installed version")?;

    if currently_installed_version.as_ref() == Some(&version) {
        return Ok(());
    }

    log::info!("missing desired version, installing rtk driver `{version}`");

    let mut install_cmd_base = Command::new("cargo");
    install_cmd_base.arg("install");

    match version {
        RtkRustcDriverVersion::CratesIo {
            major,
            minor,
            patch,
        } => {
            install_cmd_base.arg(format!("{DRIVER_NAME}@{major}.{minor}.{patch}"));
        }
        RtkRustcDriverVersion::CratesIoLatest => {
            install_cmd_base.arg(DRIVER_NAME);
        }
        RtkRustcDriverVersion::Local { path } => {
            install_cmd_base.arg("--path").arg(path);
        }
    }

    install_cmd_base
        .arg("--force")
        .arg("--locked")
        .arg("--no-track");

    let output = install_cmd_base
        .output()
        .context("failed to run cargo install command")?;

    if !output.status.success() {
        let merged_stdout_and_stderr = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);

        for line in merged_stdout_and_stderr.lines() {
            log::error!("[cargo]: {line}");
        }

        return Err(anyhow::anyhow!(
            "cargo install command failed with status: {}",
            output.status
        ));
    }

    Ok(())
}

fn currently_installed_rtk_rustc_driver_version(
    #[cfg(test)] installed_crates: &str,
) -> anyhow::Result<Option<RtkRustcDriverVersion>> {
    #[cfg(not(test))]
    let installed_crates = Command::new("cargo")
        .arg("install")
        .arg("--list")
        .output()
        .context("failed to list installed cargo packages")?
        .stdout;

    #[cfg(not(test))]
    let installed_crates = String::from_utf8(installed_crates)
        .context("failed to convert installed crates output to string")?;

    let rtk_rustc_driver_line = match installed_crates
        .lines()
        .find(|line| line.starts_with(DRIVER_NAME))
    {
        Some(l) => l,
        None => {
            return Ok(None);
        }
    };

    let maybe_local_path = rtk_rustc_driver_line
        .split_once("(")
        .and_then(|(_, path)| path.strip_suffix("):"));

    if let Some(path) = maybe_local_path {
        let path = path.trim();
        if path.is_empty() {
            return Ok(None);
        }

        return Ok(Some(RtkRustcDriverVersion::Local {
            path: PathBuf::from(path),
        }));
    }

    let parts = rtk_rustc_driver_line
        .split_whitespace()
        .collect::<Vec<&str>>();

    let version_str = parts
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("failed to parse installed RTK Rustc driver version"))?;

    let version_parts: Vec<&str> = version_str.split('.').collect();

    let major = version_parts
        .first()
        .and_then(|s| s.strip_prefix("v").unwrap().parse::<u32>().ok())
        .ok_or_else(|| anyhow::anyhow!("failed to parse major version"))?;

    let minor = version_parts
        .get(1)
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| anyhow::anyhow!("failed to parse minor version"))?;

    let patch = version_parts
        .get(2)
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| anyhow::anyhow!("failed to parse patch version"))?;

    Ok(Some(RtkRustcDriverVersion::CratesIo {
        major,
        minor,
        patch,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desired_version_for_script() {
        let script = r#"
            rtk.version("1.2.3");
            rtk.dbg_version("1.2.4");
        "#;

        let (release, debug) = desired_version_for_script(script).unwrap();
        assert_eq!(
            release,
            RtkRustcDriverVersion::CratesIo {
                major: 1,
                minor: 2,
                patch: 3
            }
        );
        assert_eq!(
            debug,
            Some(RtkRustcDriverVersion::CratesIo {
                major: 1,
                minor: 2,
                patch: 4
            })
        );
    }

    #[test]
    fn test_desired_version_for_script_latest() {
        let script = r#"
            rtk.version("latest");
        "#;

        let (release, debug) = desired_version_for_script(script).unwrap();
        assert_eq!(release, RtkRustcDriverVersion::CratesIoLatest);
        assert!(debug.is_none());
    }

    #[test]
    fn test_desired_version_for_script_local() {
        let script = r#"
            rtk.version("local:/path/to/driver");
        "#;

        let (release, _debug) = desired_version_for_script(script).unwrap();
        assert_eq!(
            release,
            RtkRustcDriverVersion::Local {
                path: PathBuf::from("/path/to/driver")
            }
        )
    }

    #[test]
    fn test_parse_invalid_version() {
        let script = r#"
            rtk.version("invalid");
        "#;

        let result = desired_version_for_script(script);
        assert!(result.is_err());
    }

    #[test]
    fn version_must_be_specified_once() {
        let script = r#"
            rtk.version("1.2.3");
            rtk.version("1.2.4");
        "#;

        let result = desired_version_for_script(script);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Lua script attempted to set the desired version multiple times, the desired version should be specified first and once"
        );
    }

    #[test]
    fn must_specify_a_version() {
        let script = r#"
        "#;

        let result = desired_version_for_script(script);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "No version was set in the Lua script"
        );
    }

    #[test]
    fn test_parse_cargo_installed_version_local() {
        let version = currently_installed_rtk_rustc_driver_version(
            r#"
rtk-rustc-driver v0.1.0 (/Developer/rtk/crates/rtk-rustc-driver):
"#,
        )
        .unwrap();

        assert!(version.is_some());
        assert_eq!(
            version.unwrap(),
            RtkRustcDriverVersion::Local {
                path: PathBuf::from("/Developer/rtk/crates/rtk-rustc-driver")
            }
        );
    }

    #[test]
    fn test_parse_cargo_installed_version_crates_io() {
        let version = currently_installed_rtk_rustc_driver_version(
            r#"
rtk-rustc-driver v0.1.0
"#,
        )
        .unwrap();

        assert!(version.is_some());
        assert_eq!(
            version.unwrap(),
            RtkRustcDriverVersion::CratesIo {
                major: 0,
                minor: 1,
                patch: 0
            }
        );
    }

    #[test]
    fn test_parse_cargo_installed_version_empty() {
        let version = currently_installed_rtk_rustc_driver_version("").unwrap();
        assert!(version.is_none());
    }
}
