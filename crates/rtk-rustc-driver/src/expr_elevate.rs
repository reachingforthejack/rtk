use rustc_ast::LitKind;
use rustc_data_structures::fx::FxHashSet;
use rustc_hir::ExprKind;
use rustc_middle::ty::{TyCtxt, TyKind};
use rustc_span::source_map::Spanned;

use crate::{
    path::{self, def_path_of_expr},
    rtk::HirIdItemIdExt,
    type_elevate::type_as_rtk_lua_type_value,
};

/// Given a rustc expr, elevate it into its simpler, lua form. This is the crux of this crate and
/// where I'd imagine most complexity lies!
pub fn as_rtk_lua_value(tcx: TyCtxt<'_>, expr: &rustc_hir::Expr<'_>) -> Option<rtk_lua::Value> {
    match expr.kind {
        ExprKind::Lit(Spanned {
            node: LitKind::Str(sym, _cooked_or_raw),
            ..
        }) => Some(rtk_lua::Value::StringLiteral(sym.to_string())),
        ExprKind::MethodCall(_path, receiver, args, _span) => {
            let parent = as_rtk_lua_value(tcx, receiver)
                .and_then(|v| match v {
                    rtk_lua::Value::MethodCall(mc) => Some(mc.origin),
                    _ => None,
                })
                .map(Box::new);

            let def_path = def_path_of_expr(tcx, expr)?;

            Some(rtk_lua::Value::MethodCall(rtk_lua::MethodCall {
                origin: rtk_lua::MethodCallQuery {
                    location: path::def_path_to_rtk_location(tcx, &def_path),
                    parent,
                },
                args: args
                    .iter()
                    .filter_map(|arg| as_rtk_lua_value(tcx, arg))
                    .collect(),
                in_item_id: expr.hir_id.rtk_item_id(),
            }))
        }
        ExprKind::Call(call_expr, args) => {
            let def_path = def_path_of_expr(tcx, call_expr)?;
            Some(rtk_lua::Value::FunctionCall(rtk_lua::FunctionCall {
                location: path::def_path_to_rtk_location(tcx, &def_path),
                args: args
                    .iter()
                    .filter_map(|arg| as_rtk_lua_value(tcx, arg))
                    .collect(),
                in_item_id: expr.hir_id.rtk_item_id(),
            }))
        }
        ExprKind::Closure(closure) => {
            let closure_ty = tcx.type_of(closure.def_id.to_def_id());

            let TyKind::Closure(_did, closure_args) = closure_ty.skip_binder().kind() else {
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
                    .filter_map(|arg| {
                        type_as_rtk_lua_type_value(
                            tcx,
                            arg.skip_binder(),
                            &mut FxHashSet::default(),
                        )
                    })
                    .collect(),
                return_type: type_as_rtk_lua_type_value(
                    tcx,
                    &o.skip_binder(),
                    &mut FxHashSet::default(),
                )
                .map(Box::new),
            };
            Some(rtk_lua::Value::Type(rtk_lua::TypeValue::Closure(ctv)))
        }
        _ => {
            let res = tcx.typeck(expr.hir_id.owner);
            type_as_rtk_lua_type_value(tcx, &res.expr_ty(expr), &mut FxHashSet::default())
                .map(rtk_lua::Value::Type)
        }
    }
}
