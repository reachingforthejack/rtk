use rustc_hir::definitions::DefPath;
use rustc_middle::ty::TyCtxt;

/// From an expr, typecheck the owner and derive the full def path
pub fn def_path_of_expr(tcx: TyCtxt<'_>, expr: &rustc_hir::Expr<'_>) -> Option<DefPath> {
    let typeck = tcx.typeck(expr.hir_id.owner);

    match typeck.type_dependent_def_id(expr.hir_id) {
        Some(did) => Some(tcx.def_path(did)),
        None => {
            let rustc_hir::ExprKind::Path(qpath) = expr.kind else {
                return None;
            };

            let qpath_res = typeck.qpath_res(&qpath, expr.hir_id);
            Some(tcx.def_path(qpath_res.def_id()))
        }
    }
}

pub fn def_path_to_rtk_location(tcx: TyCtxt<'_>, dp: &DefPath) -> rtk_lua::Location {
    let (path, impl_block_number) = dp.data.iter().fold(
        (vec![], None),
        |(mut module_path, impl_block_number), segment| match segment.data {
            rustc_hir::definitions::DefPathData::Impl if impl_block_number.is_none() => {
                (module_path, Some(segment.disambiguator as usize))
            }
            rustc_hir::definitions::DefPathData::Impl => {
                tcx.dcx()
                    .fatal("deeply nested impl blocks currently unsupported");
            }
            _ => {
                module_path.push(segment.data.to_string());
                (module_path, impl_block_number)
            }
        },
    );

    rtk_lua::Location {
        crate_name: tcx.crate_name(dp.krate).to_string(),
        path,
        impl_block_number,
    }
}

pub fn fmt_rtk_location(loc: &rtk_lua::Location) -> String {
    let impl_block = if let Some(impl_block_number) = loc.impl_block_number {
        format!("{{impl#{impl_block_number}}}")
    } else {
        String::new()
    };

    format!("{}::{}{}", loc.crate_name, loc.path.join("::"), impl_block)
}
