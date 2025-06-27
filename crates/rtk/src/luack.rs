use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use anyhow::Context;
use rtk_lua::{RtkLuaScriptExecutor, RtkRustcDriverVersion};

pub fn ck_lua(script: &str) -> anyhow::Result<()> {
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

    Ok(())
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
