//! This crate implements the Lua scripting engine for RTK that defines how users can implement RTK
//! systems for their own languages.

mod api;
mod ext;
mod macros;
mod versioning;

use anyhow::Context;
pub use api::{
    Attribute, ClosureTypeValue, EnumTypeValue, EnumTypeValueVariant, FunctionCall,
    FunctionTypeValue, Location, MethodCall, MethodCallQuery, RtkLuaScriptExecutor,
    StructTypeValue, StructTypeValueField, TraitImpl, TypeValue, Value,
};
pub use mlua::Either;
use mlua::{LuaOptions, StdLib};
pub use versioning::RtkRustcDriverVersion;

pub struct RtkLua {
    lua: mlua::Lua,
}

impl RtkLua {
    pub fn new(exec: impl RtkLuaScriptExecutor) -> anyhow::Result<Self> {
        let lua = unsafe { mlua::Lua::unsafe_new_with(StdLib::ALL, LuaOptions::new()) };

        let api = lua.create_table().context("failed to create api table")?;
        api::inject(&lua, &api, exec).context("failed to inject api into table")?;

        lua.globals()
            .set("rtk", api)
            .context("failed to set rtk api in preload")?;

        Ok(RtkLua { lua })
    }

    pub fn execute(&self, script: &str) -> anyhow::Result<()> {
        self.lua.load(script).exec()?;

        Ok(())
    }
}
