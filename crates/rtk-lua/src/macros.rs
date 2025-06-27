/// Implements `IntoLua` for a struct.
///
/// ```rust,ignore
/// impl_into_lua! {
///     FunctionTypeValue {
///         location,
///         args_struct,
///         // closure will apply to whatever the field name itself is
///         return_type => return_type.map(|b| *b),
///         item_id,
///     }
/// }
/// ```
#[macro_export]
macro_rules! impl_into_lua {
    (
        $ty:ty {
            $( $field:ident $(=> $conv:expr)? ),* $(,)?
        }
    ) => {
        impl IntoLua for $ty {
            fn into_lua(self, lua: &::mlua::Lua) -> ::mlua::Result<::mlua::Value> {
                let Self { $( $field ),* } = self;

                let table = lua.create_table()?;

                $(
                    impl_into_lua!(@assign table $field $(=> $conv)?);
                )*

                Ok(::mlua::Value::Table(table))
            }
        }
    };

    (@assign $tbl:ident $field:ident) => {
        $tbl.set(stringify!($field), $field)?;
    };

    (@assign $tbl:ident $field:ident => $conv:expr) => {
        $tbl.set(stringify!($field), $conv)?;
    };
}

/// Implements `IntoLua` for enums by mapping each variant to a Lua table formed like
/// `{ variant_name, variant_data }`
///
/// ```rust,ignore
/// #[derive(Clone, Debug)]
/// pub enum MyValue {
///     Nil,
///     Str(String),
/// }
///
/// impl_enum_into_lua! {
///     MyValue {
///         Nil,
///         // this will just forward the one tuple variant forwards
///         Str(s) => s,
///     }
/// }
/// ```
#[macro_export]
macro_rules! impl_enum_into_lua {
    (
        $enum:ident {
            $(
                $name:ident
                    $( ( $($tuple_pat:pat),* ) )?
                    $( { $($struct_pat:pat),* } )?
                    $( => $data:expr )?
            ),* $(,)?
        }
    ) => {
        impl ::mlua::IntoLua for $enum {
            fn into_lua(self, lua: &::mlua::Lua) -> ::mlua::Result<::mlua::Value> {
                match self {
                    $(
                        $enum::$name
                            $( ( $($tuple_pat),* ) )?
                            $( { $($struct_pat),* } )?
                            => {
                                let tbl = lua.create_table()?;
                                tbl.set("variant_name", stringify!($name))?;
                                tbl.set(
                                    "variant_data",
                                    impl_enum_into_lua!(@value lua $(, $data)?)
                                )?;
                                Ok(::mlua::Value::Table(tbl))
                            }
                    ),*
                }
            }
        }
    };

    (@value $lua:ident , $val:expr) => { $val };
    (@value $lua:ident) => { ::mlua::Value::Nil };
}
