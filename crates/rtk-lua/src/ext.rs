use mlua::{FromLuaMulti, IntoLua};

/// Wrapper trait around lua function settings that automatically creates the function in lua.
/// Additionally, this acts as the pinned marker for method calls that will induce the dogfooded
/// (is that a word?) rtk Lua script to emit the API types
pub trait TableSetFnExt {
    fn set_rtk_api_fn<F, I, O>(&self, lua: &mlua::Lua, key: &'static str, f: F) -> mlua::Result<()>
    where
        F: Fn(I) -> O + Send + Sync + 'static,
        I: FromLuaMulti,
        O: IntoLua;
}

impl TableSetFnExt for mlua::Table {
    fn set_rtk_api_fn<F, I, O>(&self, lua: &mlua::Lua, key: &'static str, f: F) -> mlua::Result<()>
    where
        F: Fn(I) -> O + Send + Sync + 'static,
        I: FromLuaMulti,
        O: IntoLua,
    {
        let function = lua.create_function(move |_, a: I| {
            let result = f(a);
            Ok(result)
        })?;

        self.set(key, function)
    }
}
