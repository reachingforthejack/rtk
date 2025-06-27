use rustc_ast::tokenstream::TokenTree;
use rustc_data_structures::fx::FxHashSet;
use rustc_hir::def_id::DefId;
use rustc_middle::{
    query::Key,
    ty::{Ty, TyCtxt, TyKind},
};
use rustc_type_ir::{AliasTyKind, FloatTy, IntTy, UintTy};

use crate::path;

pub fn hir_type_as_rtk_lua_type_value<'tcx>(
    tcx: TyCtxt<'tcx>,
    ty: &rustc_hir::Ty<'tcx>,
    is_async: bool,
    visited: &mut FxHashSet<(DefId, &rustc_middle::ty::GenericArgsRef<'tcx>)>,
) -> Option<rtk_lua::TypeValue> {
    let ty = tcx.type_of(ty.hir_id.owner);
    let ty = if is_async {
        peel_future_output(tcx, &ty.skip_binder())
    } else {
        ty.skip_binder()
    };
    type_as_rtk_lua_type_value(tcx, &ty, visited)
}

pub fn type_as_rtk_lua_type_value<'tcx>(
    tcx: TyCtxt<'tcx>,
    ty: &Ty<'tcx>,
    visited: &mut FxHashSet<(DefId, &rustc_middle::ty::GenericArgsRef<'tcx>)>,
) -> Option<rtk_lua::TypeValue> {
    match ty.kind() {
        TyKind::Bool => Some(rtk_lua::TypeValue::Bool),

        TyKind::Int(IntTy::I8) => Some(rtk_lua::TypeValue::I8),
        TyKind::Int(IntTy::I16) => Some(rtk_lua::TypeValue::I16),
        TyKind::Int(IntTy::I32) => Some(rtk_lua::TypeValue::I32),
        TyKind::Int(IntTy::I64) => Some(rtk_lua::TypeValue::I64),
        TyKind::Int(IntTy::I128) => Some(rtk_lua::TypeValue::I128),
        TyKind::Int(IntTy::Isize) => Some(rtk_lua::TypeValue::Isize),

        TyKind::Uint(UintTy::U8) => Some(rtk_lua::TypeValue::U8),
        TyKind::Uint(UintTy::U16) => Some(rtk_lua::TypeValue::U16),
        TyKind::Uint(UintTy::U32) => Some(rtk_lua::TypeValue::U32),
        TyKind::Uint(UintTy::U64) => Some(rtk_lua::TypeValue::U64),
        TyKind::Uint(UintTy::U128) => Some(rtk_lua::TypeValue::U128),
        TyKind::Uint(UintTy::Usize) => Some(rtk_lua::TypeValue::Usize),

        TyKind::Float(FloatTy::F32) => Some(rtk_lua::TypeValue::F32),
        TyKind::Float(FloatTy::F64) => Some(rtk_lua::TypeValue::F64),

        // if we have a reference, we just peel the reference back and then recurse on ourselves.
        // probably will be worth adding a mode for detecting references, though, but for now i
        // can't think of a great reason or need for this
        TyKind::Ref(_, ty, _) => type_as_rtk_lua_type_value(tcx, ty, visited),

        TyKind::Tuple(tys) => Some(rtk_lua::TypeValue::Tuple(
            tys.iter()
                .filter_map(|ty| type_as_rtk_lua_type_value(tcx, &ty, visited))
                .collect(),
        )),

        TyKind::Str => Some(rtk_lua::TypeValue::String),

        TyKind::Adt(adt_def, generic_args) => {
            adt_type_as_rtk_lua_type_value(tcx, adt_def, generic_args, visited)
        }

        TyKind::Closure(closure_def_id, _generic_args) => {
            let closure_ty = tcx.type_of(closure_def_id);
            let TyKind::Closure(_, closure_args) = closure_ty.skip_binder().kind() else {
                tcx.dcx()
                    .err(format!("expected closure type, found `{closure_ty:#?}`"));
                return None;
            };

            let sig = closure_args.as_closure().sig();
            let sig = tcx.signature_unclosure(sig, rustc_hir::Safety::Safe);
            let (i, o) = (sig.inputs(), sig.output());

            let ctv = rtk_lua::ClosureTypeValue {
                args: i
                    .iter()
                    .filter_map(|arg| type_as_rtk_lua_type_value(tcx, arg.skip_binder(), visited))
                    .collect(),
                return_type: type_as_rtk_lua_type_value(tcx, &o.skip_binder(), visited)
                    .map(Box::new),
            };
            Some(rtk_lua::TypeValue::Closure(ctv))
        }

        TyKind::FnDef(fn_def_id, _generic_args) => {
            let fn_sig = tcx.fn_sig(fn_def_id).skip_binder();
            let (i, o) = (fn_sig.inputs(), fn_sig.output());

            let is_async = tcx.asyncness(fn_def_id).is_async();

            let o = if is_async {
                peel_future_output(tcx, &o.skip_binder())
            } else {
                o.skip_binder()
            };

            Some(rtk_lua::TypeValue::Function(rtk_lua::FunctionTypeValue {
                is_async,
                args_struct: rtk_lua::StructTypeValue {
                    location: path::def_path_to_rtk_location(tcx, &tcx.def_path(*fn_def_id)),
                    fields: i
                        .iter()
                        .enumerate()
                        .filter_map(|(i, value)| {
                            Some(rtk_lua::StructTypeValueField {
                                name: rtk_lua::Either::Left(i),
                                // function args can't have doc comments or else clippy yells at
                                // you, so its not even worth checking!
                                doc_comment: None,
                                value: type_as_rtk_lua_type_value(
                                    tcx,
                                    value.skip_binder(),
                                    visited,
                                )?,
                                attributes: value
                                    .skip_binder()
                                    .key_as_def_id()
                                    .map(|did| attributes_for_did(tcx, did))
                                    .unwrap_or_default(),
                            })
                        })
                        .collect(),
                    attributes: attributes_for_did(tcx, *fn_def_id),
                    doc_comment: doc_comment_for_did(tcx, *fn_def_id),
                },
                location: path::def_path_to_rtk_location(tcx, &tcx.def_path(*fn_def_id)),
                return_type: type_as_rtk_lua_type_value(tcx, &o, visited).map(Box::new),
                item_id: String::new(),
                attributes: attributes_for_did(tcx, *fn_def_id),
                doc_comment: doc_comment_for_did(tcx, *fn_def_id),
            }))
        }

        _ty => None,
    }
}

fn adt_type_as_rtk_lua_type_value<'tcx>(
    tcx: TyCtxt<'tcx>,
    adt_def: &rustc_middle::ty::AdtDef<'tcx>,
    generic_args: &'tcx rustc_middle::ty::GenericArgsRef<'tcx>,
    visited: &mut FxHashSet<(DefId, &rustc_middle::ty::GenericArgsRef<'tcx>)>,
) -> Option<rtk_lua::TypeValue> {
    let def_path = tcx.def_path(adt_def.did());
    let def_path = path::def_path_to_rtk_location(tcx, &def_path);
    let fmt_def_path = path::fmt_rtk_location(&def_path);

    if let Some(known_type) =
        maybe_resolve_known_def_path(tcx, &fmt_def_path, generic_args, visited)
    {
        return Some(known_type);
    }

    if !visited.insert((adt_def.did(), generic_args)) {
        return Some(rtk_lua::TypeValue::RecursiveRef(def_path));
    }

    if adt_def.is_union() {
        tcx.dcx().err(format!(
            "encountered a union type `{fmt_def_path}` in a query"
        ));
        return None;
    }

    if adt_def.is_enum() {
        enum_type_as_rtk_lua_type_value(tcx, adt_def, generic_args, visited)
    } else {
        struct_type_as_rtk_lua_type_value(
            tcx,
            adt_def.all_fields(),
            adt_def.did(),
            generic_args,
            visited,
        )
    }
}

fn enum_type_as_rtk_lua_type_value<'tcx>(
    tcx: TyCtxt<'tcx>,
    adt_def: &rustc_middle::ty::AdtDef<'tcx>,
    generic_args: &rustc_middle::ty::GenericArgsRef<'tcx>,
    visited: &mut FxHashSet<(DefId, &rustc_middle::ty::GenericArgsRef<'tcx>)>,
) -> Option<rtk_lua::TypeValue> {
    let mut rtk_lua_variants = vec![];

    let location = path::def_path_to_rtk_location(tcx, &tcx.def_path(adt_def.did()));

    for variant in adt_def.variants() {
        let variant_fields_as_struct = struct_type_as_rtk_lua_type_value(
            tcx,
            variant.fields.iter(),
            adt_def.did(),
            generic_args,
            visited,
        );

        let rtk_lua_variant = rtk_lua::EnumTypeValueVariant {
            value: variant_fields_as_struct,
            name: variant.name.to_string(),
            attributes: attributes_for_did(tcx, variant.def_id),
            doc_comment: doc_comment_for_did(tcx, variant.def_id),
        };

        rtk_lua_variants.push(rtk_lua_variant);
    }

    Some(rtk_lua::TypeValue::Enum(rtk_lua::EnumTypeValue {
        location,
        variants: rtk_lua_variants,
        attributes: attributes_for_did(tcx, adt_def.did()),
        doc_comment: doc_comment_for_did(tcx, adt_def.did()),
    }))
}

fn struct_type_as_rtk_lua_type_value<'tcx>(
    tcx: TyCtxt<'tcx>,
    fields: impl Iterator<Item = &'tcx rustc_middle::ty::FieldDef>,
    did: DefId,
    generic_args: &rustc_middle::ty::GenericArgsRef<'tcx>,
    visited: &mut FxHashSet<(DefId, &rustc_middle::ty::GenericArgsRef<'tcx>)>,
) -> Option<rtk_lua::TypeValue> {
    let mut rtk_lua_fields = vec![];

    for (i, field) in fields.enumerate() {
        let field_ident = field.ident(tcx);
        let field_ident = if field_ident.is_numeric() {
            rtk_lua::Either::Left(i)
        } else {
            rtk_lua::Either::Right(field_ident.to_string())
        };

        let field_ty = field.ty(tcx, generic_args);

        match type_as_rtk_lua_type_value(tcx, &field_ty, visited) {
            Some(value) => {
                let rtk_lua_field = rtk_lua::StructTypeValueField {
                    name: field_ident,
                    value,
                    attributes: attributes_for_did(tcx, field.did),
                    doc_comment: doc_comment_for_did(tcx, field.did),
                };

                rtk_lua_fields.push(rtk_lua_field);
            }
            None => {
                tcx.dcx().warn(
                    format!(
                        "encountered an field type `{field_ty:#?}` in a query, \
                         the rest of the fields will still be attempted but this one will be skipped."
                    ),
                );
            }
        }
    }

    Some(rtk_lua::TypeValue::Struct(rtk_lua::StructTypeValue {
        location: path::def_path_to_rtk_location(tcx, &tcx.def_path(did)),
        fields: rtk_lua_fields,
        attributes: attributes_for_did(tcx, did),
        doc_comment: doc_comment_for_did(tcx, did),
    }))
}

fn maybe_resolve_known_def_path<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_path: &str,
    generic_args: &rustc_middle::ty::GenericArgsRef<'tcx>,
    visited: &mut FxHashSet<(DefId, &rustc_middle::ty::GenericArgsRef<'tcx>)>,
) -> Option<rtk_lua::TypeValue> {
    match def_path {
        "alloc::boxed::Box" => generic_args
            .iter()
            .next()
            .and_then(|arg| type_as_rtk_lua_type_value(tcx, &arg.expect_ty(), visited)),
        "core::option::Option" => generic_args
            .iter()
            .next()
            .and_then(|arg| type_as_rtk_lua_type_value(tcx, &arg.expect_ty(), visited))
            .map(Box::new)
            .map(rtk_lua::TypeValue::Option),
        "core::result::Result" => {
            let mut generic_args = generic_args.iter();
            let ok_type = generic_args
                .next()
                .and_then(|arg| type_as_rtk_lua_type_value(tcx, &arg.expect_ty(), visited))
                .map(Box::new)?;
            let err_type = generic_args
                .next()
                .and_then(|arg| type_as_rtk_lua_type_value(tcx, &arg.expect_ty(), visited))
                .map(Box::new)?;

            Some(rtk_lua::TypeValue::Result(ok_type, err_type))
        }
        "hashbrown::map::HashMap" | "std::collections::hash::map::HashMap" => {
            let mut generic_args = generic_args.iter();
            let key_type = generic_args
                .next()
                .and_then(|arg| type_as_rtk_lua_type_value(tcx, &arg.expect_ty(), visited))
                .map(Box::new)?;
            let value_type = generic_args
                .next()
                .and_then(|arg| type_as_rtk_lua_type_value(tcx, &arg.expect_ty(), visited))
                .map(Box::new)?;

            Some(rtk_lua::TypeValue::HashMap(key_type, value_type))
        }
        "alloc::string::String" => Some(rtk_lua::TypeValue::String),
        "alloc::vec::Vec" => {
            // vecs have two args, with the second being the allocator. we only care about the
            // first `T` so the rest of the generic args are redundant
            generic_args
                .iter()
                .next()
                .and_then(|arg| type_as_rtk_lua_type_value(tcx, &arg.expect_ty(), visited))
                .map(Box::new)
                .map(rtk_lua::TypeValue::Vec)
        }
        _ => None,
    }
}

pub fn attributes_for_did(tcx: TyCtxt, did: DefId) -> Vec<rtk_lua::Attribute> {
    let attrs = tcx.get_attrs_unchecked(did);

    let mut proc_macro_attributes = vec![];
    for attr in attrs
        .iter()
        .filter(|a| matches!(a.kind, rustc_hir::AttrKind::Normal(_)))
    {
        let name = attr.name_or_empty().to_string();
        let value_str = match &attr.kind {
            rustc_hir::AttrKind::Normal(ai) => match &ai.args {
                rustc_hir::AttrArgs::Empty => String::new(),
                rustc_hir::AttrArgs::Eq { eq_span: _, expr } => expr.symbol.to_string(),
                rustc_hir::AttrArgs::Delimited(delim_args) => {
                    pretty_print_delimited_token_stream(&delim_args.tokens)
                }
            },
            rustc_hir::AttrKind::DocComment(_, _) => {
                unreachable!()
            }
        };

        proc_macro_attributes.push(rtk_lua::Attribute {
            name,
            value_str: Some(value_str),
        });
    }

    proc_macro_attributes
}

fn pretty_print_delimited_token_stream(toks: &rustc_ast::tokenstream::TokenStream) -> String {
    toks.iter()
        .map(|token| match token {
            TokenTree::Token(token, _spacing) => match token.kind {
                rustc_ast::token::TokenKind::Literal(l) => l.to_string(),
                rustc_ast::token::TokenKind::Ident(ident, _) => ident.to_string(),
                rustc_ast::token::TokenKind::Eq => "=".to_string(),
                rustc_ast::token::TokenKind::Comma => ",".to_string(),
                rustc_ast::token::TokenKind::Colon => ":".to_string(),
                rustc_ast::token::TokenKind::Semi => ";".to_string(),
                _ => String::new(),
            },
            TokenTree::Delimited(_span, _spacing, _delim, ts) => {
                pretty_print_delimited_token_stream(ts)
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

pub fn doc_comment_for_did(tcx: TyCtxt, did: DefId) -> Option<String> {
    let doc = tcx.get_attrs_unchecked(did);
    if doc.is_empty() {
        return None;
    }

    let doc = doc
        .iter()
        .filter_map(|attr| match attr.kind {
            rustc_hir::AttrKind::DocComment(_cc, sym) => Some(sym.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if doc.is_empty() { None } else { Some(doc) }
}

pub fn peel_future_output<'tcx>(tcx: TyCtxt<'tcx>, ty: &Ty<'tcx>) -> Ty<'tcx> {
    match ty.kind() {
        TyKind::Alias(AliasTyKind::Opaque, alias_ty) => {
            let ty = tcx.type_of_opaque(alias_ty.def_id).unwrap();
            peel_future_output(tcx, &ty.skip_binder())
        }
        TyKind::Coroutine(_, generic_args) => {
            // first three args are coroutine bootstrapping, fourth is the output, and 5 + 6 hold the body
            // and input args
            let fut_output = generic_args.get(3).unwrap();
            fut_output.expect_ty()
        }
        _ => tcx
            .dcx()
            .fatal(format!("expected coroutine type, found `{ty:#?}`")),
    }
}
