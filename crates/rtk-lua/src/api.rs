use anyhow::Context;
use mlua::{Either, FromLua, IntoLua, Lua};

use crate::{
    ext::TableSetFnExt, impl_enum_into_lua, impl_into_lua, versioning::RtkRustcDriverVersion,
};

pub trait RtkLuaScriptExecutor: Send + Sync + Clone + 'static {
    /// Intake the driver version provided by the script
    fn intake_version(&self, version: RtkRustcDriverVersion);

    /// Intake the driver version provided by the script to use when debug assertions are enabled
    fn intake_debug_version(&self, version: RtkRustcDriverVersion) {
        self.intake_version(version);
    }

    fn query_method_calls(&self, query: MethodCallQuery) -> Vec<MethodCall>;
    fn query_trait_impls(&self, query: Location) -> Vec<TraitImpl>;
    fn query_functions(&self, query: Location) -> Vec<FunctionTypeValue>;
    fn query_function_calls(&self, query: Location) -> Vec<FunctionCall>;

    fn log_note(&self, msg: String);
    fn log_warn(&self, msg: String);
    fn log_error(&self, msg: String);
    fn log_fatal_error(&self, msg: String) -> !;

    fn emit(&self, text: String);
}

/// Injects the full API into the table
pub fn inject(
    lua: &Lua,
    table: &mlua::Table,
    exec: impl RtkLuaScriptExecutor,
) -> anyhow::Result<()> {
    let intake_version_exec = exec.clone();

    table
        .set_rtk_api_fn(lua, "version", move |version: RtkRustcDriverVersion| {
            intake_version_exec.intake_version(version);
            mlua::Nil
        })
        .context("failed to set intake_version function")?;

    let intake_debug_version_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "dbg_version", move |version: RtkRustcDriverVersion| {
            intake_debug_version_exec.intake_debug_version(version);
            mlua::Nil
        })
        .context("failed to set intake_debug_version function")?;

    let note_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "note", move |msg: String| {
            note_exec.log_note(msg);
            mlua::Nil
        })
        .context("failed to set debug function")?;

    let warn_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "warn", move |msg: String| {
            warn_exec.log_warn(msg);
            mlua::Nil
        })
        .context("failed to set warn function")?;

    let error_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "error", move |msg: String| {
            error_exec.log_error(msg);
            mlua::Nil
        })
        .context("failed to set error function")?;

    let fatal_error_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "fatal_error", move |msg: String| {
            fatal_error_exec.log_fatal_error(msg);
            // required or else we get annoying warnings about the return type
            #[allow(unreachable_code)]
            mlua::Nil
        })
        .context("failed to set fatal_error function")?;

    let query_method_calls_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "query_method_calls", move |query: MethodCallQuery| {
            query_method_calls_exec.query_method_calls(query)
        })
        .context("failed to set query_method_calls function")?;

    let query_trait_impls_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "query_trait_impls", move |query: Location| {
            query_trait_impls_exec.query_trait_impls(query)
        })
        .context("failed to set query_trait_impls function")?;

    let query_functions_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "query_functions", move |query: Location| {
            query_functions_exec.query_functions(query)
        })
        .context("failed to set query_functions function")?;

    let query_function_calls_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "query_function_calls", move |query: Location| {
            query_function_calls_exec.query_function_calls(query)
        })
        .context("failed to set query_function_calls function")?;

    let emit_exec = exec.clone();
    table
        .set_rtk_api_fn(lua, "emit", move |text: String| {
            emit_exec.emit(text);
            mlua::Nil
        })
        .context("failed to set emit function")?;

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Location {
    pub crate_name: String,
    pub path: Vec<String>,
    pub impl_block_number: Option<usize>,
}

impl FromLua for Location {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        let table = value
            .as_table()
            .ok_or_else(|| mlua::Error::FromLuaConversionError {
                from: "Value",
                to: "Location".to_string(),
                message: Some("expected a table".to_string()),
            })?;

        let crate_name: String = table.get("crate_name")?;
        let path: Vec<String> = table.get("path")?;
        let impl_block_number: Option<usize> = table.get("impl_block_number")?;

        Ok(Location {
            crate_name,
            path,
            impl_block_number,
        })
    }
}

impl_into_lua! {
    Location {
        crate_name,
        path,
        impl_block_number,
    }
}

/// A query for method calls matching a specific path.
/// This can be used, for example, to look for axum routes
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodCallQuery {
    /// If specified, this requires this method call to originate from a prior parent method call.
    /// For instance with the given source:
    /// ```rust,ignore
    /// my_var.globals().set("something", 1).set("something_else", 2);
    /// ```
    /// By setting `parent` to the method call query of `globals`, we can enforce that the
    /// set call is in a chain of `globals` and not a set on some other table.
    pub parent: Option<Box<MethodCallQuery>>,
    /// The path to the module this method call sits in.
    pub location: Location,
}

impl_into_lua! {
    MethodCallQuery {
        parent => parent.map(|b| *b),
        location,
    }
}

impl FromLua for MethodCallQuery {
    fn from_lua(value: mlua::Value, _lua: &mlua::Lua) -> mlua::Result<Self> {
        let table = value
            .as_table()
            .ok_or_else(|| mlua::Error::FromLuaConversionError {
                from: "Value",
                to: "MethodCallQuery".to_string(),
                message: Some("expected a table".to_string()),
            })?;

        let parent = match table.get("parent")? {
            mlua::Nil => None,
            t => Some(Box::new(Self::from_lua(t, _lua)?)),
        };

        let location: Location =
            table
                .get("location")
                .map_err(|_| mlua::Error::FromLuaConversionError {
                    from: "Value",
                    to: "Location".to_string(),
                    message: Some("expected a Location".to_string()),
                })?;

        Ok(MethodCallQuery { parent, location })
    }
}

#[derive(Clone, Debug)]
pub struct MethodCall {
    /// The query that produced this method call. This won't always be your own query, as certain
    /// situations will cause one to be automatically generated. For instance, if you make a method
    /// call query one of the arguments to it can be another method call.
    pub origin: MethodCallQuery,
    pub args: Vec<Value>,
    pub in_item_id: String,
}

impl_into_lua! {
    MethodCall {
        origin,
        args,
        in_item_id,
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    StringLiteral(String),
    IntegerLiteral(i64),
    FloatLiteral(f64),

    FunctionCall(FunctionCall),
    MethodCall(MethodCall),

    Type(TypeValue),
}

impl_enum_into_lua! {
    Value {
        StringLiteral(s) => s,
        IntegerLiteral(i) => i,
        FloatLiteral(f) => f,

        FunctionCall(f) => f,
        MethodCall(m) => m,

        Type(t) => t,
    }
}

#[derive(Clone, Debug)]
pub enum TypeValue {
    String,

    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,

    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,

    F32,
    F64,

    Bool,

    HashMap(Box<TypeValue>, Box<TypeValue>),
    Vec(Box<TypeValue>),
    Result(Box<TypeValue>, Box<TypeValue>),

    Struct(StructTypeValue),
    Enum(EnumTypeValue),

    Closure(ClosureTypeValue),
    Function(FunctionTypeValue),

    Option(Box<TypeValue>),

    Tuple(Vec<TypeValue>),

    RecursiveRef(Location),
}

impl_enum_into_lua! {
    TypeValue {
        String,
        U8,
        U16,
        U32,
        U64,
        U128,
        Usize,
        I8,
        I16,
        I32,
        I64,
        I128,
        Isize,
        F32,
        F64,
        Bool,

        // HashMap(k, v) => (*k, *v),
        HashMap(_, _) => mlua::Nil,
        Vec(t) => *t,
        // Result(ok, err) => (*ok, *err),
        Result(_, _) => mlua::Nil,

        Struct(s) => s,
        Enum(e) => e,

        Closure(c) => c,

        Function(f) => f,

        Option(t) => *t,

        Tuple(elements) => elements,

        RecursiveRef(location) => location,
    }
}

#[derive(Clone, Debug)]
pub struct StructTypeValue {
    pub location: Location,
    pub fields: Vec<StructTypeValueField>,
    pub doc_comment: Option<String>,
    pub attributes: Vec<Attribute>,
}

impl_into_lua! {
    StructTypeValue {
        location,
        fields,
        doc_comment,
        attributes,
    }
}

#[derive(Clone, Debug)]
pub struct StructTypeValueField {
    pub name: Either<usize, String>,
    pub doc_comment: Option<String>,
    pub attributes: Vec<Attribute>,
    pub value: TypeValue,
}

impl_into_lua! {
    StructTypeValueField {
        name,
        doc_comment,
        attributes,
        value,
    }
}

#[derive(Clone, Debug)]
pub struct EnumTypeValue {
    pub location: Location,
    pub variants: Vec<EnumTypeValueVariant>,
    pub doc_comment: Option<String>,
    pub attributes: Vec<Attribute>,
}

impl_into_lua! {
    EnumTypeValue {
        location,
        variants,
        doc_comment,
        attributes,
    }
}

#[derive(Clone, Debug)]
pub struct EnumTypeValueVariant {
    pub name: String,
    /// If this variant has a value, this will be the type of that value otherwise its just a unit
    /// variant
    pub value: Option<TypeValue>,
    pub doc_comment: Option<String>,
    pub attributes: Vec<Attribute>,
}

impl_into_lua! {
    EnumTypeValueVariant {
        name,
        value,
        doc_comment,
        attributes,
    }
}

/// A closure definition itself. The args are just a struct ultimately
#[derive(Clone, Debug)]
pub struct ClosureTypeValue {
    pub args: Vec<TypeValue>,
    pub return_type: Option<Box<TypeValue>>,
}

impl_into_lua! {
    ClosureTypeValue {
        args,
        return_type => return_type.map(|b| *b),
    }
}

#[derive(Clone, Debug)]
pub struct FunctionTypeValue {
    pub location: Location,
    pub args_struct: StructTypeValue,
    pub return_type: Option<Box<TypeValue>>,
    pub item_id: String,
    pub attributes: Vec<Attribute>,
    pub doc_comment: Option<String>,
    pub is_async: bool,
}

impl_into_lua! {
    FunctionTypeValue {
        location,
        args_struct,
        return_type => return_type.map(|b| *b),
        item_id,
        attributes,
        doc_comment,
        is_async,
    }
}

/// An attribute in the source code.
#[derive(Clone, Debug)]
pub struct Attribute {
    pub name: String,
    // in the case of a rename, this will be `"my_name"` _NOT_ `my_name`
    pub value_str: Option<String>,
}

impl_into_lua! {
    Attribute {
        name,
        value_str,
    }
}

#[derive(Clone, Debug)]
pub struct FunctionCall {
    pub location: Location,
    pub args: Vec<Value>,
    pub in_item_id: String,
}

impl_into_lua! {
    FunctionCall {
        location,
        args,
        in_item_id,
    }
}

#[derive(Clone, Debug)]
pub struct TraitImpl {
    pub trait_location: Location,
    pub for_type: TypeValue,
    pub functions: Vec<FunctionTypeValue>,
}

impl_into_lua! {
    TraitImpl {
        trait_location,
        for_type,
        functions,
    }
}
