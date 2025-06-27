#![feature(rustc_private)]
#![warn(clippy::correctness, clippy::perf, clippy::style, clippy::suspicious)]

mod expr_elevate;
mod path;
mod queries;
mod rtk;
mod type_elevate;

// use callbacks::{DefaultCallbacks, KindInertiaTsCallbacks};
use rustc_driver::{Callbacks, catch_with_exit_code, run_compiler};
use rustc_session::{EarlyDiagCtxt, config::ErrorOutputType};
use std::process::ExitCode;

extern crate either;
extern crate itertools;
extern crate parking_lot;
extern crate rustc_ast;
extern crate rustc_ast_pretty;
extern crate rustc_codegen_ssa;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_error_codes;
extern crate rustc_errors;
extern crate rustc_hash;
extern crate rustc_hir;
extern crate rustc_hir_pretty;
extern crate rustc_interface;
extern crate rustc_lexer;
extern crate rustc_metadata;
extern crate rustc_middle;
extern crate rustc_serialize;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;
extern crate rustc_type_ir;
extern crate thin_vec;

fn main() -> ExitCode {
    let early_dcx = EarlyDiagCtxt::new(ErrorOutputType::default());

    rustc_driver::init_rustc_env_logger(&early_dcx);

    let exit_code = catch_with_exit_code(move || {
        let mut args = rustc_driver::args::raw_args(&early_dcx);
        // always skip arg0 which is the path to the binary itself (i.e. rustc if not primary, or riptc if primary)
        args.remove(0);

        let is_primary = std::env::var("CARGO_PRIMARY_PACKAGE").is_ok();
        if is_primary {
            let lua_script_path = std::env::var("RTK_LUA_SCRIPT").expect(
                "missing `RTK_LUA_SCRIPT` env var, you are likely not running through the cli",
            );
            let out_file_path = std::env::var("RTK_OUT_FILE").expect(
                "missing `RTK_OUT_FILE` env var, you are likely not running through the cli",
            );

            run_compiler(
                &args,
                &mut rtk::RtkCallbacks {
                    lua_script_path,
                    out_file_path,
                },
            );
        } else {
            // if this is a dependency or build script, just forward to the regular compiler without any
            // of our hooks. this will still allow the primary package to analyze, for instance, types and serde
            // attributes of workspace dependencies because they are a part of the primary package build but its
            // a waste of time to run our callbacks when we're not primary!

            run_compiler(&args, &mut DefaultCallbacks);
        }
    });

    ExitCode::from(exit_code as u8)
}

pub struct DefaultCallbacks;

impl Callbacks for DefaultCallbacks {}
