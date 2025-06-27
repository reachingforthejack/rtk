use rustc_data_structures::fx::FxHashSet;
use rustc_hir::{ExprKind, ImplItemKind, ItemKind};
use rustc_middle::ty::TyCtxt;

use crate::{
    expr_elevate,
    path::{self, fmt_rtk_location},
    rtk::HirIdItemIdExt,
    type_elevate::{attributes_for_did, doc_comment_for_did, hir_type_as_rtk_lua_type_value},
};

pub fn method_call_from_expr(
    tcx: TyCtxt<'_>,
    mc: &rtk_lua::MethodCallQuery,
    expr: &rustc_hir::Expr<'_>,
) -> Option<rtk_lua::MethodCall> {
    let (reciever, args, _span) = match expr.kind {
        ExprKind::MethodCall(_path_seg, rx, args, span) => (*rx, args.iter().copied(), span),
        _ => return None,
    };

    if let Some(mcq) = &mc.parent {
        // TODO: this needs to walk up the call chain, currently this just enforces direct parents
        let _ = method_call_from_expr(tcx, mcq, &reciever)?;
    }

    let def_path = path::def_path_of_expr(tcx, expr)?;
    let def_path_loc = path::def_path_to_rtk_location(tcx, &def_path);

    if def_path_loc != mc.location {
        if def_path_loc.path.last() == mc.location.path.last() {
            tcx.dcx().warn(
                format!(
                    "query for `{}` likely intended to match against `{}`, consider changing the impl block number",
                    fmt_rtk_location(&mc.location),
                    fmt_rtk_location(&def_path_loc),
                ),
            );
        }

        return None;
    }

    let args = args
        .filter_map(|arg| expr_elevate::as_rtk_lua_value(tcx, &arg))
        .collect();

    let mc = rtk_lua::MethodCall {
        origin: mc.clone(),
        args,
        in_item_id: expr.hir_id.rtk_item_id(),
    };

    Some(mc)
}

pub fn trait_impl_from_item<'tcx>(
    tcx: TyCtxt<'tcx>,
    location: &rtk_lua::Location,
    item: &rustc_hir::Item<'tcx>,
) -> Option<rtk_lua::TraitImpl> {
    let ItemKind::Impl(i) = item.kind else {
        return None;
    };

    let of_trait = i.of_trait?;
    let def_path = tcx.def_path(of_trait.trait_def_id().unwrap());

    if &path::def_path_to_rtk_location(tcx, &def_path) != location {
        return None;
    }

    let for_type =
        match hir_type_as_rtk_lua_type_value(tcx, i.self_ty, false, &mut FxHashSet::default()) {
            Some(t) => t,
            None => {
                tcx.dcx()
                    .span_warn(item.span, "failed to convert self type");
                return None;
            }
        };

    let functions = i.items.iter().filter_map(|item| {
        let impl_item = tcx.hir_impl_item(item.id);
        match impl_item.kind {
            ImplItemKind::Const(_, _) => {
                tcx.dcx().span_warn(
                    item.span,
                    "trait impls cannot contain const items currently",
                );
                None
            }
            ImplItemKind::Type(_) => {
                tcx.dcx()
                    .span_warn(item.span, "trait impls cannot contain type items currently");
                None
            }
            ImplItemKind::Fn(sig, body_id) => fn_sig_into_rtk_function_value_type(
                tcx,
                impl_item.owner_id,
                &body_id,
                location,
                &sig,
            ),
        }
    });

    Some(rtk_lua::TraitImpl {
        trait_location: location.clone(),
        for_type,
        functions: functions.collect(),
    })
}

pub fn function_from_item<'tcx>(
    tcx: TyCtxt<'tcx>,
    location: &rtk_lua::Location,
    item: &rustc_hir::Item<'tcx>,
) -> Option<rtk_lua::FunctionTypeValue> {
    let ItemKind::Fn {
        sig,
        generics,
        body,
        has_body,
    } = item.kind
    else {
        return None;
    };

    if !generics.params.is_empty() {
        tcx.dcx().span_warn(
            item.span,
            "function generic parameters will be ignored (may be elided lifetimes or synthetic impl generics)",
        );
    }

    if !has_body {
        tcx.dcx()
            .span_warn(item.span, "function without body cannot be queried");
        return None;
    }

    let def_path = tcx.def_path(item.owner_id.def_id.to_def_id());
    if &path::def_path_to_rtk_location(tcx, &def_path) != location {
        return None;
    }

    fn_sig_into_rtk_function_value_type(tcx, item.owner_id, &body, location, &sig)
}

// TODO: consolidate this better with the type elevation module
fn fn_sig_into_rtk_function_value_type<'tcx>(
    tcx: TyCtxt<'tcx>,
    owner_id: rustc_hir::OwnerId,
    body_id: &rustc_hir::BodyId,
    loc: &rtk_lua::Location,
    sig: &rustc_hir::FnSig<'tcx>,
) -> Option<rtk_lua::FunctionTypeValue> {
    let is_async = tcx.asyncness(owner_id.def_id.to_def_id()).is_async();
    let args_struct_fields = sig
        .decl
        .inputs
        .iter()
        .enumerate()
        .filter_map(|(i, arg)| {
            let value =
                hir_type_as_rtk_lua_type_value(tcx, arg, is_async, &mut FxHashSet::default())?;

            Some(rtk_lua::StructTypeValueField {
                name: rtk_lua::Either::Left(i),
                attributes: vec![],
                value,
                doc_comment: None,
            })
        })
        .collect();

    let args_struct = rtk_lua::StructTypeValue {
        location: loc.clone(),
        fields: args_struct_fields,
        attributes: attributes_for_did(tcx, owner_id.def_id.to_def_id()),
        doc_comment: doc_comment_for_did(tcx, owner_id.def_id.to_def_id()),
    };

    let function_def_path = tcx.def_path(owner_id.def_id.to_def_id());
    let location = path::def_path_to_rtk_location(tcx, &function_def_path);

    let is_async = tcx.asyncness(owner_id.def_id.to_def_id()).is_async();
    let return_type = match sig.decl.output {
        rustc_hir::FnRetTy::DefaultReturn(_) => None,
        rustc_hir::FnRetTy::Return(ty) => {
            hir_type_as_rtk_lua_type_value(tcx, ty, is_async, &mut FxHashSet::default())
        }
    }
    .map(Box::new);

    Some(rtk_lua::FunctionTypeValue {
        is_async,
        location,
        return_type,
        args_struct,
        item_id: body_id.hir_id.rtk_item_id(),
        attributes: attributes_for_did(tcx, owner_id.def_id.to_def_id()),
        doc_comment: doc_comment_for_did(tcx, owner_id.def_id.to_def_id()),
    })
}

pub fn function_call_from_expr(
    tcx: TyCtxt<'_>,
    loc: &rtk_lua::Location,
    expr: &rustc_hir::Expr<'_>,
) -> Option<rtk_lua::FunctionCall> {
    let ExprKind::Call(call_expr, args) = expr.kind else {
        return None;
    };

    let def_path = path::def_path_of_expr(tcx, call_expr)?;
    let def_path_loc = path::def_path_to_rtk_location(tcx, &def_path);

    if &def_path_loc != loc {
        return None;
    }

    let args = args
        .iter()
        .filter_map(|arg| expr_elevate::as_rtk_lua_value(tcx, arg))
        .collect();

    Some(rtk_lua::FunctionCall {
        location: def_path_loc,
        args,
        in_item_id: expr.hir_id.rtk_item_id(),
    })
}
