//! C++ projection: the thunks that implement `bwapi_c.h` over the BWAPI headers.
//!
//! Every thunk is `noexcept`, checks the declared return type with
//! `static_assert` before converting it to its C type, and passes arguments
//! without casts so that `-Wconversion -Werror` rejects narrowing. Handles cross
//! as opaque C pointers: a thunk casts them to and from the BWAPI class the
//! manifest names. Set iteration is hand-written in `cpp/sets.h` and `cpp/sets.cpp`.

use std::fmt::Write;

use crate::c;
use crate::ir::*;
use crate::names::snake;
use crate::{GENERATED_HEADER, OutputFile, append};

pub fn generate(api: &Api) -> Vec<OutputFile> {
    let mut files = Vec::new();
    let header = format!("{GENERATED_HEADER}\n#include \"../sets.h\"\n");
    let mut emit = |owner: &str, text: String| {
        append(
            &mut files,
            format!("{}.cpp", snake(owner)),
            &header,
            &format!("\n{text}"),
        )
    };

    for class in api.handles.iter().map(|h| &h.class) {
        let recv = Recv::Pointer {
            c: c::receiver(class, true),
            cpp: &class.cpp,
        };
        emit(&class.name.0, class_text(api, class, &recv));
    }
    for class in &api.type_ids {
        let recv = Recv::Id {
            c: c::receiver(class, false),
            cpp: &class.cpp,
        };
        emit(&class.name.0, class_text(api, class, &recv));
    }
    for s in &api.statics {
        let mut out = consts(&s.cpp, &s.consts);
        for f in &s.functions {
            thunk(&mut out, api, &Recv::Static(&s.cpp), f);
        }
        skips(&mut out, &s.cpp, &s.skips);
        emit(&s.owner.0, out);
    }
    for e in &api.enums {
        emit(&e.name.0, consts(&e.cpp, &e.variants));
    }
    files
}

enum Recv<'a> {
    /// `Unit* self` → `reinterpret_cast<BWAPI::UnitInterface*>(self)->m(…)`.
    Pointer { c: String, cpp: &'a str },
    /// `UnitType self` → `const BWAPI::UnitType recv(self); recv.m(…)`.
    Id { c: String, cpp: &'a str },
    /// No receiver → `ns::f(…)`.
    Static(&'a str),
}

fn class_text(api: &Api, class: &Class, recv: &Recv) -> String {
    let mut out = String::new();
    for m in &class.methods {
        thunk(&mut out, api, recv, m);
    }
    skips(&mut out, &class.cpp, &class.skips);
    out
}

fn consts(scope: &str, consts: &[Const]) -> String {
    let mut out = String::new();
    for c in consts {
        let name = &c.cpp.0;
        writeln!(
            out,
            "static_assert(static_cast<int>({scope}::{name}) == {}, \"bwapi.api: wrong value of {scope}::{name}\");",
            c.value
        )
        .unwrap();
    }
    out
}

fn skips(out: &mut String, cpp: &str, skips: &[Skip]) {
    for s in skips {
        writeln!(out, "// skipped {cpp}::{}: {}", s.cpp.0, s.reason).unwrap();
    }
}

/// The C++ type BWAPI must use for a marker: checked with `static_assert`.
fn expected(api: &Api, t: &Type) -> String {
    match t {
        Type::Bool => "bool".into(),
        Type::I32 => "int".into(),
        Type::U32 => "unsigned".into(),
        Type::F64 => "double".into(),
        Type::Handle { name, .. } => format!("{}*", api.handle(name).class.cpp),
        Type::Value(v) => v.cpp().into(),
        Type::TypeId(n) => api.type_id(n).cpp.clone(),
        Type::Enum(n) => api.enum_(n).cpp.clone(),
        Type::CxxString => "std::string".into(),
        Type::Pair(a, b) => format!("std::pair<{}, {}>", expected(api, a), expected(api, b)),
        Type::Set(h) | Type::Query(h) => api.set_class(h).into(),
        _ => unreachable!("no single expected C++ type"),
    }
}

/// Converts a BWAPI value `e` to its C type.
fn to_c(api: &Api, t: &Type, e: &str) -> String {
    match t {
        Type::I32 => format!("static_cast<int32_t>({e})"),
        Type::U32 => format!("static_cast<uint32_t>({e})"),
        Type::TypeId(_) => format!("static_cast<int32_t>({e}.getID())"),
        Type::Value(v) => format!("{}{{{e}.x, {e}.y}}", v.rust()),
        Type::Handle { .. } => format!("reinterpret_cast<{}>({e})", c::c_type(api, t)),
        Type::Pair(a, b) => format!(
            "Pair{{{}, {}}}",
            to_c(api, a, &format!("{e}.first")),
            to_c(api, b, &format!("{e}.second"))
        ),
        _ => e.to_owned(),
    }
}

fn thunk(out: &mut String, api: &Api, recv: &Recv, m: &Method) {
    let mut prelude = String::new();
    let mut call_args = Vec::new();
    let (c_recv, target) = match recv {
        Recv::Pointer { c, cpp } => (
            Some(c.as_str()),
            format!("reinterpret_cast<{cpp}*>(self)->{}", m.cpp.0),
        ),
        Recv::Id { c, cpp } => {
            // A named object, not a temporary: a returned reference never points
            // into an object destroyed at the end of the statement.
            writeln!(prelude, "  const {cpp} recv(self);").unwrap();
            (Some(c.as_str()), format!("recv.{}", m.cpp.0))
        }
        Recv::Static(ns) => (None, format!("{ns}::{}", m.cpp.0)),
    };
    let sig = c::signature(api, c_recv, m);

    // Omitted arguments at the tail are not passed: C++ applies the header default.
    let passed = m
        .args
        .iter()
        .rposition(|a| a.omitted.is_none())
        .map_or(0, |i| i + 1);
    for (i, a) in m.args.iter().enumerate() {
        let p = &c::param(a);
        if let Some(expr) = &a.omitted {
            if i < passed {
                call_args.push(expr.clone());
            }
            continue;
        }
        call_args.push(match &a.ty {
            Type::Bool | Type::I32 | Type::U32 | Type::F64 => p.clone(),
            Type::Handle { name, .. } => {
                format!("reinterpret_cast<{}*>({p})", api.handle(name).class.cpp)
            }
            Type::Value(v) => format!("{}({p}.x, {p}.y)", v.cpp()),
            Type::TypeId(n) => format!("{}({p})", api.type_id(n).cpp),
            Type::Enum(n) => format!("static_cast<{}>({p})", api.enum_(n).cpp),
            Type::CxxString | Type::CStr | Type::Fmt => {
                writeln!(prelude, "  const std::string {p}Str({p}, {p}Len);").unwrap();
                match a.ty {
                    Type::CxxString => format!("{p}Str"),
                    Type::CStr => format!("{p}Str.c_str()"),
                    // A user string is never a format.
                    _ => format!("\"%s\", {p}Str.c_str()"),
                }
            }
            _ => unreachable!("lowering restricts argument markers"),
        });
    }

    let call = format!("{target}({})", call_args.join(", "));
    let get = format!("  decltype(auto) ret = {call};\n");
    let check = |e: String| {
        format!(
            "  static_assert(bwapi_c::same_v<decltype(ret), {e}>, \"bwapi.api: wrong return type of {}\");\n",
            m.cpp.0
        )
    };
    let body = match &m.ret {
        // Returning a value from a void function does not compile: the void return is checked too.
        Type::Void => format!("  return {call};\n"),
        Type::Bool
        | Type::F64
        | Type::Handle { .. }
        | Type::I32
        | Type::U32
        | Type::TypeId(_)
        | Type::Value(_)
        | Type::Pair(..) => format!(
            "{get}{}  return {};\n",
            check(expected(api, &m.ret)),
            to_c(api, &m.ret, "ret")
        ),
        Type::CxxString => format!(
            "{get}{}  bwapi_c::write_string(out, ret);\n",
            check(expected(api, &m.ret))
        ),
        Type::Vec(elem) => format!(
            "{get}  static_assert(bwapi_c::same_v<bwapi_c::elem_t<decltype(ret)>, {}>, \"bwapi.api: wrong element type of {}\");\n  bwapi_c::write_vec<{}>(out, ret, [](const auto& e) {{ return {}; }});\n",
            expected(api, elem),
            m.cpp.0,
            c::slice_elem(api, &m.ret),
            to_c(api, elem, "e")
        ),
        Type::Set(_) => format!(
            "{get}  static_assert(std::is_lvalue_reference_v<decltype(ret)>, \"bwapi.api: {} must return a set by reference\");\n{}  return reinterpret_cast<{}>(&ret);\n",
            m.cpp.0,
            check(expected(api, &m.ret)),
            sig.ret
        ),
        Type::Query(h) => format!(
            "{get}  static_assert(!std::is_reference_v<decltype(ret)>, \"bwapi.api: {} must return a set by value\");\n{}  return reinterpret_cast<{}>(new bwapi_c::QueryState<{}>(std::move(ret)));\n",
            m.cpp.0,
            check(expected(api, &m.ret)),
            sig.ret,
            api.set_class(h)
        ),
        _ => unreachable!("lowering rejects this return"),
    };
    writeln!(
        out,
        "{} {}({}) noexcept {{\n{prelude}{body}}}",
        sig.ret,
        m.c_name,
        sig.params.join(", ")
    )
    .unwrap();
}
