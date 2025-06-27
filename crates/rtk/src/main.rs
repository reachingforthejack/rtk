mod versioning;

use anyhow::Context;
use clap::Parser;
use std::{path::PathBuf, process::Command};

const DRIVER_NAME: &str = "rtk-rustc-driver";

/// RTK CLI. Query your Rust types, and emit bindings for anything with no macros!
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The input Lua script file to use for the RTK driver.
    #[arg(short, long)]
    script: PathBuf,

    /// The output file for where calls to `rtk.emit` in the Lua script will write to.
    #[arg(short, long)]
    out_file: PathBuf,

    /// Additional arguments to pass to `cargo`. RTK wraps `cargo check`, so you can forward any
    /// additional arguments here such as `-p <your-crate>` to only target a specific crate.
    #[arg(last = true)]
    cargo_args: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    let script_src =
        std::fs::read_to_string(&args.script).context("failed to read input Lua script")?;

    let (driver_release_version, driver_debug_version) =
        versioning::desired_version_for_script(&script_src)
            .context("failed to extract desired version from Lua script")?;

    let driver_version = if cfg!(debug_assertions) {
        driver_debug_version.unwrap_or(driver_release_version)
    } else {
        driver_release_version
    };

    versioning::install_rtk_rustc_driver(driver_version)
        .context("failed to install RTK Rustc driver")?;

    log::info!("driver version provisioned / already installed, proceeding with cargo execution");

    Command::new("cargo")
        .env("RUSTC_WRAPPER", DRIVER_NAME)
        .env("RTK_LUA_SCRIPT", &args.script)
        .env("RTK_OUT_FILE", &args.out_file)
        .arg("check")
        .args(args.cargo_args)
        .status()
        .context("failed to execute cargo check")?;

    Ok(())
}
