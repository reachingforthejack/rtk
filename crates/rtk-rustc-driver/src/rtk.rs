use std::{io::Write, sync::Arc};

use rtk_lua::{MethodCallQuery, RtkLua, RtkLuaScriptExecutor};
use rustc_driver::{Callbacks, Compilation};
use rustc_hir::{
    Expr,
    intravisit::{Visitor, nested_filter::NestedFilter},
};
use rustc_middle::ty::TyCtxt;

use crate::queries;

pub struct RtkCallbacks {
    pub lua_script_path: String,
    pub out_file_path: String,
}

impl Callbacks for RtkCallbacks {
    fn after_analysis(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        tcx: rustc_middle::ty::TyCtxt<'_>,
    ) -> rustc_driver::Compilation {
        let out_file_handle = match std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.out_file_path)
        {
            Ok(handle) => Arc::new(parking_lot::Mutex::new(handle)),
            Err(e) => {
                tcx.dcx().fatal(format!(
                    "failed to open output file '{}': {e}",
                    self.out_file_path
                ));
            }
        };

        let lua = RtkLua::new(unsafe {
            std::mem::transmute::<
                RtkLuaScriptVisitorExecutor<'_>,
                RtkLuaScriptVisitorExecutor<'static>,
            >(RtkLuaScriptVisitorExecutor {
                tcx,
                out_file_handle,
            })
        })
        .unwrap();

        let lua_script = match std::fs::read_to_string(&self.lua_script_path) {
            Ok(script) => script,
            Err(e) => {
                tcx.dcx().fatal(format!(
                    "failed to read Lua script from '{}': {e}",
                    self.lua_script_path
                ));
            }
        };

        if let Err(err) = lua.execute(&lua_script) {
            tcx.dcx()
                .fatal(format!("Lua script execution failed: {err}"));
        }

        Compilation::Stop
    }
}

pub struct VisitorFilter;

impl<'tcx> NestedFilter<'tcx> for VisitorFilter {
    type MaybeTyCtxt = TyCtxt<'tcx>;

    const INTER: bool = true;
    const INTRA: bool = true;
}

#[derive(Clone)]
struct RtkLuaScriptVisitorExecutor<'tcx> {
    tcx: TyCtxt<'tcx>,
    out_file_handle: Arc<parking_lot::Mutex<std::fs::File>>,
}

unsafe impl Send for RtkLuaScriptVisitorExecutor<'_> {}
unsafe impl Sync for RtkLuaScriptVisitorExecutor<'_> {}

impl RtkLuaScriptExecutor for RtkLuaScriptVisitorExecutor<'static> {
    fn intake_version(&self, _version: rtk_lua::RtkRustcDriverVersion) {
        // TODO: assert version matches self in here
    }

    fn query_method_calls(&self, query: MethodCallQuery) -> Vec<rtk_lua::MethodCall> {
        struct MCVisitor<'tcx> {
            tcx: TyCtxt<'tcx>,
            calls: Vec<rtk_lua::MethodCall>,
            query: MethodCallQuery,
        }

        impl<'tcx> Visitor<'tcx> for MCVisitor<'tcx> {
            type NestedFilter = VisitorFilter;

            fn visit_expr(&mut self, ex: &'tcx Expr<'tcx>) {
                if let Some(mc) = queries::method_call_from_expr(self.tcx, &self.query, ex) {
                    self.calls.push(mc);
                }

                rustc_hir::intravisit::walk_expr(self, ex)
            }

            fn maybe_tcx(&mut self) -> Self::MaybeTyCtxt {
                self.tcx
            }
        }

        let mut mc_visitor = MCVisitor {
            tcx: self.tcx,
            calls: Vec::new(),
            query,
        };

        self.tcx.hir_walk_toplevel_module(&mut mc_visitor);

        mc_visitor.calls
    }

    fn query_trait_impls(&self, query: rtk_lua::Location) -> Vec<rtk_lua::TraitImpl> {
        struct TIVisitor<'tcx> {
            tcx: TyCtxt<'tcx>,
            traits: Vec<rtk_lua::TraitImpl>,
            location: rtk_lua::Location,
        }

        impl<'tcx> Visitor<'tcx> for TIVisitor<'tcx> {
            type NestedFilter = VisitorFilter;

            fn visit_item(&mut self, i: &'tcx rustc_hir::Item<'tcx>) -> Self::Result {
                if let Some(ti) = queries::trait_impl_from_item(self.tcx, &self.location, i) {
                    self.traits.push(ti);
                }

                rustc_hir::intravisit::walk_item(self, i);
            }

            fn maybe_tcx(&mut self) -> Self::MaybeTyCtxt {
                self.tcx
            }
        }

        let mut ti_visitor = TIVisitor {
            tcx: self.tcx,
            traits: Vec::new(),
            location: query,
        };

        self.tcx.hir_walk_toplevel_module(&mut ti_visitor);

        ti_visitor.traits
    }

    fn query_functions(&self, query: rtk_lua::Location) -> Vec<rtk_lua::FunctionTypeValue> {
        struct FVisitor<'tcx> {
            tcx: TyCtxt<'tcx>,
            functions: Vec<rtk_lua::FunctionTypeValue>,
            location: rtk_lua::Location,
        }

        impl<'tcx> Visitor<'tcx> for FVisitor<'tcx> {
            type NestedFilter = VisitorFilter;

            fn visit_item(&mut self, i: &'tcx rustc_hir::Item<'tcx>) -> Self::Result {
                if let Some(ti) = queries::function_from_item(self.tcx, &self.location, i) {
                    self.functions.push(ti);
                }

                rustc_hir::intravisit::walk_item(self, i);
            }

            fn maybe_tcx(&mut self) -> Self::MaybeTyCtxt {
                self.tcx
            }
        }

        let mut f_visitor = FVisitor {
            tcx: self.tcx,
            functions: Vec::new(),
            location: query,
        };

        self.tcx.hir_walk_toplevel_module(&mut f_visitor);

        f_visitor.functions
    }

    fn query_function_calls(&self, query: rtk_lua::Location) -> Vec<rtk_lua::FunctionCall> {
        struct FCVisitor<'tcx> {
            tcx: TyCtxt<'tcx>,
            calls: Vec<rtk_lua::FunctionCall>,
            location: rtk_lua::Location,
        }

        impl<'tcx> Visitor<'tcx> for FCVisitor<'tcx> {
            type NestedFilter = VisitorFilter;

            fn visit_expr(&mut self, ex: &'tcx Expr<'tcx>) {
                if let Some(fc) = queries::function_call_from_expr(self.tcx, &self.location, ex) {
                    self.calls.push(fc);
                }

                rustc_hir::intravisit::walk_expr(self, ex);
            }

            fn maybe_tcx(&mut self) -> Self::MaybeTyCtxt {
                self.tcx
            }
        }

        let mut fc_visitor = FCVisitor {
            tcx: self.tcx,
            calls: Vec::new(),
            location: query,
        };

        self.tcx.hir_walk_toplevel_module(&mut fc_visitor);

        fc_visitor.calls
    }

    fn log_note(&self, msg: String) {
        self.tcx.dcx().note(msg);
    }

    fn log_warn(&self, msg: String) {
        self.tcx.dcx().warn(msg);
    }

    fn log_error(&self, msg: String) {
        self.tcx.dcx().err(msg);
    }

    fn log_fatal_error(&self, msg: String) -> ! {
        self.tcx.dcx().fatal(msg);
    }

    fn emit(&self, text: String) {
        let mut handle = self.out_file_handle.lock();
        match handle.write_all(text.as_bytes()) {
            Ok(_) => {}
            Err(e) => {
                self.tcx
                    .dcx()
                    .fatal(format!("failed to write to out file: {e}",));
            }
        }
    }
}

pub trait HirIdItemIdExt {
    fn rtk_item_id(self) -> String;
}

impl HirIdItemIdExt for rustc_hir::HirId {
    fn rtk_item_id(self) -> String {
        let def_id = self.owner.to_def_id();
        format!("{}/{}", def_id.krate.as_usize(), def_id.index.as_usize())
    }
}
