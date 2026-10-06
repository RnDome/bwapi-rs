//! Rust projection: the private `raw` extern layer and the public API over it.
//!
//! Handles carry the match brand `'game` and take it from their receiver: a handle
//! returned by a method is branded like the object it was asked from. The shapes of
//! the types are hand-written macros (src/runtime/types.rs), sets are in
//! src/runtime/sets.rs.

use std::fmt::Write;

use crate::ir::*;
use crate::names::snake;
use crate::{GENERATED_HEADER, OutputFile, append};

const PRELUDE: &str = r#"#![allow(unused_imports)]

use core::ffi::c_void;

use super::*;
use crate::runtime::ffi::{PairI32, Pos, Slice};
use crate::runtime::sets::*;
use crate::{BwString, Position, TilePosition, WalkPosition};
"#;

pub fn generate(api: &Api) -> Vec<OutputFile> {
    let mut files = Vec::new();
    let mut raw = String::new();
    let header = format!("{GENERATED_HEADER}\n{PRELUDE}");
    let mut emit = |owner: &str, text: String| {
        append(&mut files, format!("{}.rs", snake(owner)), &header, &text)
    };

    for class in api.handles.iter().map(|h| &h.class) {
        let mut out = format!(
            "\ncrate::runtime::types::handle!({}, \"{}\");\n",
            class.name.0, class.cpp
        );
        methods(&mut out, &mut raw, class, Recv::Handle);
        emit(&class.name.0, out);
    }
    for class in &api.type_ids {
        let mut out = format!(
            "\ncrate::runtime::types::type_id!({}, \"{}\");\n",
            class.name.0, class.cpp
        );
        methods(&mut out, &mut raw, class, Recv::Id);
        emit(&class.name.0, out);
    }
    for s in &api.statics {
        let mut out = format!("\nimpl {} {{\n", s.owner.0);
        for c in &s.consts {
            let t = &s.owner.0;
            writeln!(
                out,
                "    /// BWAPI: `{}::{}`\n    pub const {}: {t} = {t}({});\n",
                s.cpp, c.cpp.0, c.rust.0, c.value
            )
            .unwrap();
        }
        for f in &s.functions {
            method(&mut out, &mut raw, &s.owner.0, &s.cpp, Recv::Static, f);
        }
        skip_comments(&mut out, &s.cpp, &s.skips);
        out.push_str("}\n");
        emit(&s.owner.0, out);
    }
    for e in &api.enums {
        let mut out = format!(
            "\ncrate::runtime::types::c_enum! {{\n    {}, \"{}\",\n",
            e.name.0, e.cpp
        );
        for v in &e.variants {
            writeln!(out, "    {} = {}, \"{}\";", v.rust.0, v.value, v.cpp.0).unwrap();
        }
        out.push_str("}\n");
        emit(&e.name.0, out);
    }

    let mut mod_rs = format!(
        "{GENERATED_HEADER}\n// Lints that the shape of the BWAPI API triggers in generated code.\n#![allow(\n    clippy::too_many_arguments,\n    clippy::should_implement_trait,\n    clippy::wrong_self_convention\n)]\n\npub(crate) mod raw;\n"
    );
    for f in &files {
        let m = f.path.trim_end_matches(".rs");
        writeln!(mod_rs, "\nmod {m};\npub use {m}::*;").unwrap();
    }
    files.push(OutputFile {
        path: "mod.rs".into(),
        contents: mod_rs,
    });

    let mut raw_rs = format!(
        "{GENERATED_HEADER}\n// The functions of cpp/include/bwapi_c.h, named as in C.\n#![allow(non_snake_case, unused_imports)]\n\nuse core::ffi::c_void;\n\nuse crate::runtime::ffi::{{PairI32, Pos, Slice}};\n\n"
    );
    for class in api.handles.iter().map(|h| &h.class) {
        writeln!(
            raw_rs,
            "crate::runtime::types::opaque!({}, \"{}\");",
            class.name.0, class.cpp
        )
        .unwrap();
    }
    write!(raw_rs, "\nunsafe extern \"C\" {{\n{raw}}}\n").unwrap();
    files.push(OutputFile {
        path: "raw.rs".into(),
        contents: raw_rs,
    });
    files
}

#[derive(Clone, Copy)]
enum Recv {
    /// `self` is a branded handle: returned handles take its brand.
    Handle,
    /// `self` is a typeid.
    Id,
    Static,
}

fn methods(out: &mut String, raw: &mut String, class: &Class, recv: Recv) {
    if class.methods.is_empty() && class.skips.is_empty() {
        return;
    }
    let name = &class.name.0;
    match recv {
        Recv::Handle => writeln!(out, "\nimpl<'game> {name}<'game> {{").unwrap(),
        _ => writeln!(out, "\nimpl {name} {{").unwrap(),
    }
    for m in &class.methods {
        method(out, raw, name, &class.cpp, recv, m);
    }
    skip_comments(out, &class.cpp, &class.skips);
    out.push_str("}\n");
}

fn skip_comments(out: &mut String, cpp: &str, skips: &[Skip]) {
    for s in skips {
        writeln!(out, "    // skipped `{cpp}::{}`: {}", s.cpp.0, s.reason).unwrap();
    }
}

/// Public Rust type of a marker in a return.
fn public(t: &Type) -> String {
    match t {
        Type::Void => "()".into(),
        Type::Bool => "bool".into(),
        Type::I32 => "i32".into(),
        Type::U32 => "u32".into(),
        Type::F64 => "f64".into(),
        Type::Handle {
            name,
            nullability: Nullability::Nullable,
        } => format!("Option<{name}<'game>>"),
        Type::Handle { name, .. } => format!("{name}<'game>"),
        Type::Value(v) => v.rust().into(),
        Type::TypeId(n) | Type::Enum(n) => n.clone(),
        Type::CStr | Type::Fmt => "&str".into(),
        Type::CxxString => "BwString".into(),
        Type::Vec(e) => format!("Vec<{}>", public(e)),
        Type::Pair(a, b) => format!("({}, {})", public(a), public(b)),
        Type::Set(h) => format!("{h}Set<'game>"),
        Type::Query(h) => format!("{h}Query<'game>"),
        Type::Filter => unreachable!("filters are omitted"),
    }
}

/// The Rust type of a scalar marker on the C boundary, as `raw` declares it.
fn raw_ty(t: &Type) -> String {
    match t {
        Type::Bool => "bool".into(),
        Type::I32 | Type::TypeId(_) | Type::Enum(_) => "i32".into(),
        Type::U32 => "u32".into(),
        Type::F64 => "f64".into(),
        Type::Handle { name, .. } => format!("*mut {name}"),
        Type::Value(_) => "Pos".into(),
        Type::Pair(..) => "PairI32".into(),
        Type::Set(h) => format!("*const crate::runtime::sets::Raw{h}Set"),
        Type::Query(h) => format!("*mut crate::runtime::sets::Raw{h}Query"),
        _ => unreachable!("not a scalar on the C boundary"),
    }
}

/// Converts an ABI value `e` to the public type; handles take the receiver's brand.
fn from_abi(t: &Type, e: &str) -> String {
    match t {
        Type::TypeId(n) => format!("{n}::from_id({e})"),
        Type::Value(v) => format!("{}::from({e})", v.rust()),
        Type::Handle { name, .. } => {
            format!(
                "unsafe {{ {name}::from_raw(self.brand(), {e}.cast()) }}.expect(\"BWAPI returned a null element\")"
            )
        }
        Type::Pair(a, b) => format!(
            "({}, {})",
            from_abi(a, &format!("{e}.first")),
            from_abi(b, &format!("{e}.second"))
        ),
        _ => e.to_owned(),
    }
}

fn method(out: &mut String, raw: &mut String, owner: &str, cpp: &str, recv: Recv, m: &Method) {
    let sym = &m.c_name;
    let mut sig = Vec::new();
    let mut raw_params = Vec::new();
    let mut call = Vec::new();
    match recv {
        Recv::Handle => {
            sig.push("self".to_owned());
            raw_params.push(format!("self_: *mut {owner}"));
            call.push("self.as_ptr()".to_owned());
        }
        Recv::Id => {
            sig.push("self".to_owned());
            raw_params.push("self_: i32".to_owned());
            call.push("self.0".to_owned());
        }
        Recv::Static => {}
    }
    for a in m.args.iter().filter(|a| a.omitted.is_none()) {
        let n = &a.name.0;
        if matches!(a.ty, Type::CStr | Type::CxxString | Type::Fmt) {
            sig.push(format!("{n}: &str"));
            raw_params.push(format!("{n}_ptr: *const u8, {n}_len: usize"));
            call.push(format!("{n}.as_ptr(), {n}.len()"));
            continue;
        }
        let arg = match &a.ty {
            Type::Handle {
                name,
                nullability: Nullability::Nullable,
            } => format!("{n}.map_or(core::ptr::null_mut(), {name}::as_ptr)"),
            Type::Handle { .. } => format!("{n}.as_ptr()"),
            Type::Value(_) => format!("Pos::from({n})"),
            Type::TypeId(_) => format!("{n}.id()"),
            Type::Enum(_) => format!("{n} as i32"),
            _ => n.clone(),
        };
        sig.push(format!("{n}: {}", public(&a.ty)));
        raw_params.push(format!("{n}: {}", raw_ty(&a.ty)));
        call.push(arg);
    }

    let ret = &m.ret;
    let raw_expr = |call: &[String]| format!("raw::{sym}({})", call.join(", "));
    let raw_call = |call: &[String]| format!("unsafe {{ {} }}", raw_expr(call));
    // Strings and vectors come back in a `Slice` the thunk fills.
    let mut out_call = call.clone();
    out_call.push("&mut out".into());
    let slice_call = format!("let mut out = Slice::new();\n{};\n", raw_call(&out_call));
    let body = match ret {
        Type::Void | Type::Bool | Type::I32 | Type::U32 | Type::F64 => raw_call(&call),
        Type::Handle { name, nullability } => {
            let get = format!(
                "unsafe {{ {name}::from_raw(self.brand(), {}) }}",
                raw_expr(&call)
            );
            match nullability {
                Nullability::ProvenNonNull => format!(
                    "{get}.expect(\"BWAPI violated the non-null contract of {cpp}::{}\")",
                    m.cpp.0
                ),
                _ => get,
            }
        }
        Type::Value(_) | Type::TypeId(_) | Type::Pair(..) => {
            format!("let ret = {};\n{}", raw_call(&call), from_abi(ret, "ret"))
        }
        Type::CxxString => format!(
            "{slice_call}// SAFETY: the thunk filled `out` with the bytes of a string.\nBwString::from_vec(unsafe {{ out.into_vec::<u8>() }})"
        ),
        Type::Vec(elem) => {
            // The Rust mirror of the C element type; a public module sees raw handles as `c_void`.
            let mirror = match &**elem {
                Type::Handle { .. } => "*mut c_void".to_owned(),
                e => raw_ty(e),
            };
            let vec = format!("unsafe {{ out.into_vec::<{mirror}>() }}");
            let finish = match &**elem {
                Type::I32 => vec,
                Type::TypeId(n) => format!("{vec}.into_iter().map({n}::from_id).collect()"),
                Type::Value(v) => format!("{vec}.into_iter().map({}::from).collect()", v.rust()),
                _ => format!(
                    "{vec}.into_iter().map(|e| {}).collect()",
                    from_abi(elem, "e")
                ),
            };
            format!(
                "{slice_call}// SAFETY: the thunk filled `out` with elements of `{mirror}`.\n{finish}"
            )
        }
        Type::Set(h) => format!(
            "unsafe {{ {h}Set::from_raw(self.brand(), {}) }}",
            raw_expr(&call)
        ),
        Type::Query(h) => format!(
            "unsafe {{ {h}Query::from_raw(self.brand(), {}) }}",
            raw_expr(&call)
        ),
        _ => unreachable!("lowering rejects this return"),
    };
    let raw_ret = match ret {
        Type::Void => String::new(),
        Type::CxxString | Type::Vec(_) => {
            raw_params.push("out: *mut Slice".into());
            String::new()
        }
        t => format!(" -> {}", raw_ty(t)),
    };
    writeln!(
        raw,
        "    pub(crate) fn {sym}({}){raw_ret};",
        raw_params.join(", ")
    )
    .unwrap();

    let ret_sig = match ret {
        Type::Void => String::new(),
        _ => format!(" -> {}", public(ret)),
    };
    write!(
        out,
        "    /// BWAPI: `{cpp}::{}`\n    pub fn {}({}){ret_sig} {{\n        {body}\n    }}\n\n",
        m.cpp.0,
        m.name.0,
        sig.join(", ")
    )
    .unwrap();
}
