use std::vec::Vec;

use analysis::imports::Imports;
use analysis::needed_upcast::needed_upcast;
use analysis::out_parameters;
use analysis::parameter;
use analysis::ref_mode::RefMode;
use analysis::return_value;
use analysis::rust_type::*;
use analysis::upcasts::Upcasts;
use env::Env;
use library::{self, Nullable};
use nameutil;
use config::RegexList;
use traits::*;
use version::Version;

//TODO: change use Parameter to reference?
pub struct Info {
    pub name: String,
    pub glib_name: String,
    pub kind: library::FunctionKind,
    pub comented: bool,
    pub type_name: Result,
    pub parameters: Vec<parameter::Parameter>,
    pub ret: return_value::Info,
    pub upcasts: Upcasts,
    pub outs: out_parameters::Info,
    pub version: Option<Version>,
}

pub fn analyze(env: &Env, functions: &[library::Function], type_tid: library::TypeId,
    non_nullable_overrides: &[String], ignored_functions: &RegexList,
    imports: &mut Imports) -> Vec<Info> {
    let mut funcs = Vec::new();

    for func in functions {
        if ignored_functions.is_match(&func.name) {
            continue;
        }
        let info = analyze_function(env, func, type_tid, non_nullable_overrides, imports);
        funcs.push(info);
    }

    funcs
}

fn analyze_function(env: &Env, func: &library::Function, type_tid: library::TypeId,
    non_nullable_overrides: &[String], imports: &mut Imports) -> Info {
    let mut commented = false;
    let mut upcasts: Upcasts = Default::default();
    let mut used_types: Vec<String> = Vec::with_capacity(4);

    let ret = return_value::analyze(env, func, type_tid, non_nullable_overrides, &mut used_types);
    commented |= ret.commented;

    let parameters: Vec<parameter::Parameter> =
        func.parameters.iter().map(|par| parameter::analyze(env, par)).collect();

    for (pos, par) in parameters.iter().enumerate() {
        assert!(!par.instance_parameter || pos == 0,
            "Wrong instance parameter in {}", func.c_identifier.as_ref().unwrap());
        if let Ok(s) = used_rust_type(env, par.typ) {
            used_types.push(s);
        }
        let type_error = parameter_rust_type(env, par.typ, par.direction, Nullable(false), RefMode::None).is_err();
        if !par.instance_parameter && needed_upcast(&env.library, par.typ) {
            let type_name = rust_type(env, par.typ);
            let ignored = if type_error { "/*Ignored*/" } else { "" };
            if !upcasts.add_parameter(&par.name, &format!("{}{}", ignored, type_name.as_str())) {
                panic!("Too many parameters upcasts for {}", func.c_identifier.as_ref().unwrap())
            }
        }
        if type_error {
            commented = true;
        }
    }

    let (outs, unsupported_outs) = out_parameters::analyze(env, func);
    if unsupported_outs {
        warn!("Function {} has unsupported outs", func.c_identifier.as_ref().unwrap_or(&func.name));
        commented = true;
    } else if !outs.is_empty() && !commented {
        //TODO: move to out_parameters::analyze
        imports.add("std::mem".into(), func.version);
    }

    if !commented {
        for s in used_types {
            if let Some(i) = s.find("::") {
                imports.add(s[..i].into(), func.version);
            }
            else {
                imports.add(s, func.version);
            }
        }
        if ret.base_tid.is_some() {
            imports.add("glib::object::Downcast".into(), None);
        }
        if !upcasts.is_empty() {
            imports.add("glib::object::Upcast".into(), None);
        }
    }

    Info {
        name: nameutil::mangle_keywords(&*func.name).into_owned(),
        glib_name: func.c_identifier.as_ref().unwrap().clone(),
        kind: func.kind,
        comented: commented,
        type_name: rust_type(env, type_tid),
        parameters: parameters,
        ret: ret,
        upcasts: upcasts,
        outs: outs,
        version: func.version,
    }
}
