//! C projection: `bwapi_c.h`, the C API that the C++ thunks implement.
//!
//! Every name of the header comes from the manifest, except the fixed part: value
//! structs, `Slice`, the module and its events, set iteration. The thunks
//! include the header, so the C++ compiler checks every definition against it.

use std::fmt::Write;

use crate::GENERATED_HEADER;
use crate::ir::*;
use crate::names::{c_scope, camel};

/// The C type of a scalar argument or return.
pub fn c_type(api: &Api, t: &Type) -> String {
    match t {
        Type::Bool => "bool".into(),
        Type::I32 => "int32_t".into(),
        Type::U32 => "uint32_t".into(),
        Type::F64 => "double".into(),
        Type::Handle { name, .. } => format!("{name}*"),
        Type::Value(v) => v.rust().into(),
        Type::TypeId(n) | Type::Enum(n) => n.clone(),
        Type::Pair(..) => "Pair".into(),
        Type::Set(h) => format!("const {}*", set_name(api, h)),
        Type::Query(h) => format!("{}*", query_name(h)),
        _ => unreachable!("not a scalar C type"),
    }
}

/// `Unitset` for `BWAPI::Unitset`.
pub fn set_name<'a>(api: &'a Api, handle: &str) -> &'a str {
    c_scope(api.set_class(handle))
}

pub fn query_name(handle: &str) -> String {
    format!("{handle}Query")
}

/// The C type of an element of a `Slice`: of a `Vec<T>` return, or `uint8_t` of a string.
pub fn slice_elem(api: &Api, ret: &Type) -> String {
    match ret {
        Type::CxxString => "uint8_t".into(),
        Type::Vec(elem) => c_type(api, elem),
        _ => unreachable!("only strings and vectors are returned in a Slice"),
    }
}

/// A C function: the return type and the parameters, `type name` each.
pub struct Signature {
    pub ret: String,
    pub params: Vec<String>,
    /// What the function writes to `Slice* out`, for the comment of the declaration.
    pub slice: Option<String>,
}

/// The C signature of a function of the manifest.
pub fn signature(api: &Api, recv: Option<&str>, m: &Method) -> Signature {
    let mut params = Vec::new();
    if let Some(recv) = recv {
        params.push(format!("{recv} self"));
    }
    for a in m.args.iter().filter(|a| a.omitted.is_none()) {
        let p = param(a);
        params.push(match &a.ty {
            Type::CxxString | Type::CStr | Type::Fmt => {
                format!("const char* {p}, size_t {p}Len")
            }
            t => format!("{} {p}", c_type(api, t)),
        });
    }
    let slice = match &m.ret {
        Type::CxxString => Some("uint8_t, a string without a terminating zero".to_owned()),
        Type::Vec(_) => Some(slice_elem(api, &m.ret)),
        _ => None,
    };
    let ret = match &m.ret {
        Type::Void => "void".into(),
        Type::CxxString | Type::Vec(_) => {
            params.push("Slice* out".into());
            "void".into()
        }
        t => c_type(api, t),
    };
    Signature { ret, params, slice }
}

/// The C parameter of an argument. A string is `const char* name, size_t nameLen`:
/// not null-terminated.
pub fn param(a: &Arg) -> String {
    camel(&a.name.0)
}

/// The receiver of the functions of a section, if any.
pub fn receiver(class: &Class, is_handle: bool) -> String {
    if is_handle {
        format!("{}*", class.name.0)
    } else {
        class.name.0.clone()
    }
}

const PRELUDE: &str = r#"#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
#define BWAPI_C_NOEXCEPT noexcept
extern "C" {
#else
#define BWAPI_C_NOEXCEPT
#endif

/* Values. */

typedef struct Position { int32_t x; int32_t y; } Position;
typedef struct TilePosition { int32_t x; int32_t y; } TilePosition;
typedef struct WalkPosition { int32_t x; int32_t y; } WalkPosition;
typedef struct Pair { int32_t first; int32_t second; } Pair;

/* BWAPI::Event: one callback of the module. */
typedef struct Event_ Event;
/* BWAPI::AIModule. */
typedef struct AIModule_ AIModule;

/* Room for an iterator over a set: it is moved by copying the bytes. */
typedef struct SetCursor { uint64_t words[4]; } SetCursor;

/* A string or a vector returned by the library: `len` elements of the type the
   function names. `data` comes from bwapi_c_alloc_slice, or is null when `len`
   is 0; the caller owns it and frees it. */
typedef struct Slice { void* data; size_t len; } Slice;
"#;

const REQUIRED: &str = r#"
/* Required: the user of this library defines these functions. */

/* Memory for `count` elements of `elemSize` bytes aligned to `elemAlign`: the
   data of a Slice. Called only with `count` > 0; must not return null. */
void* bwapi_c_alloc_slice(size_t count, size_t elemSize, size_t elemAlign) BWAPI_C_NOEXCEPT;

/* Every callback of a module created by bwapi_c_new_module. The event lives for
   the duration of the call. */
void bwapi_c_on_event(void* module, Game* game, const Event* event) BWAPI_C_NOEXCEPT;
"#;

const MODULE: &str = r#"

/* Module. */

/* For `gameInit` of the bot DLL: sets BWAPI::BroodwarPtr. */
void bwapi_c_game_init(Game* game) BWAPI_C_NOEXCEPT;
/* For `newAIModule` of the bot DLL: a module passing its callbacks to bwapi_c_on_event. */
AIModule* bwapi_c_new_module(void* module) BWAPI_C_NOEXCEPT;

/* Event: which fields are set depends on the type. */

EventType Event_getType(const Event* self) BWAPI_C_NOEXCEPT;
Unit* Event_getUnit(const Event* self) BWAPI_C_NOEXCEPT;
Player* Event_getPlayer(const Event* self) BWAPI_C_NOEXCEPT;
Position Event_getPosition(const Event* self) BWAPI_C_NOEXCEPT;
/* The bytes belong to the event. */
void Event_getText(const Event* self, const char** ptr, size_t* len) BWAPI_C_NOEXCEPT;
bool Event_isWinner(const Event* self) BWAPI_C_NOEXCEPT;
"#;

const SETS: &str = r#"
/*
 * Sets. `{Set}` is a live set owned by the game; `{Query}` is a set returned by
 * value, owned by the caller until {Query}_release. Iteration: `atEnd` reports
 * exhaustion, `next` requires `!atEnd`.
 */
"#;

pub fn generate(api: &Api) -> String {
    let mut out = format!("{GENERATED_HEADER}\n{PRELUDE}");

    out.push_str("\n/* Handles: BWAPI interfaces, used only by pointer. */\n\n");
    for h in &api.handles {
        let n = &h.class.name.0;
        writeln!(out, "typedef struct {n}_ {n}; /* {} */", h.class.cpp).unwrap();
    }
    for h in api.handles.iter().filter(|h| h.set.is_some()) {
        let cpp = api.set_class(&h.class.name.0);
        let set = c_scope(cpp);
        let query = query_name(&h.class.name.0);
        writeln!(out, "typedef struct {set}_ {set}; /* const {cpp}& */").unwrap();
        writeln!(out, "typedef struct {query}_ {query}; /* {cpp} by value */").unwrap();
    }

    out.push_str("\n/* Type ids and enums: the integer value of the C++ type. */\n\n");
    for t in &api.type_ids {
        writeln!(out, "typedef int32_t {}; /* {} */", t.name.0, t.cpp).unwrap();
    }
    for e in &api.enums {
        writeln!(out, "typedef int32_t {}; /* {} */", e.name.0, e.cpp).unwrap();
    }

    out.push_str(REQUIRED);
    out.push_str(MODULE);

    out.push_str(SETS);
    for h in api.handles.iter().filter(|h| h.set.is_some()) {
        let elem = &h.class.name.0;
        let set = set_name(api, elem);
        let query = query_name(elem);
        write!(
            out,
            "\n\
             size_t {set}_size(const {set}* self) BWAPI_C_NOEXCEPT;\n\
             bool {set}_contains(const {set}* self, {elem}* elem) BWAPI_C_NOEXCEPT;\n\
             void {set}_begin(const {set}* self, SetCursor* cursor) BWAPI_C_NOEXCEPT;\n\
             bool {set}Cursor_atEnd(const SetCursor* self) BWAPI_C_NOEXCEPT;\n\
             {elem}* {set}Cursor_next(SetCursor* self) BWAPI_C_NOEXCEPT;\n\
             size_t {set}Cursor_remaining(const SetCursor* self) BWAPI_C_NOEXCEPT;\n\
             bool {query}_atEnd(const {query}* self) BWAPI_C_NOEXCEPT;\n\
             {elem}* {query}_next({query}* self) BWAPI_C_NOEXCEPT;\n\
             size_t {query}_remaining(const {query}* self) BWAPI_C_NOEXCEPT;\n\
             void {query}_release({query}* self) BWAPI_C_NOEXCEPT;\n"
        )
        .unwrap();
    }

    for h in &api.handles {
        functions(&mut out, api, &h.class, Some(&receiver(&h.class, true)));
    }
    for t in &api.type_ids {
        functions(&mut out, api, t, Some(&receiver(t, false)));
    }
    for s in &api.statics {
        if !s.functions.is_empty() {
            writeln!(out, "\n/* {} */\n", s.cpp).unwrap();
            for f in &s.functions {
                declaration(&mut out, api, None, f);
            }
        }
    }

    out.push_str("\n/* Constants. */\n");
    for s in api.statics.iter().filter(|s| !s.consts.is_empty()) {
        constants(
            &mut out,
            c_scope(&s.cpp),
            s.consts.iter().map(|c| (&c.cpp.0, c.value)),
        );
    }
    for e in &api.enums {
        constants(
            &mut out,
            &e.name.0,
            e.variants.iter().map(|c| (&c.cpp.0, c.value)),
        );
    }

    out.push_str("\n#ifdef __cplusplus\n}  /* extern \"C\" */\n#endif\n");
    out
}

fn functions(out: &mut String, api: &Api, class: &Class, recv: Option<&str>) {
    if class.methods.is_empty() {
        return;
    }
    writeln!(out, "\n/* {} */\n", class.cpp).unwrap();
    for m in &class.methods {
        declaration(out, api, recv, m);
    }
}

fn declaration(out: &mut String, api: &Api, recv: Option<&str>, m: &Method) {
    let sig = signature(api, recv, m);
    let params = if sig.params.is_empty() {
        "void".to_owned()
    } else {
        sig.params.join(", ")
    };
    let comment = sig
        .slice
        .map(|elem| format!(" /* out: {elem} */"))
        .unwrap_or_default();
    writeln!(
        out,
        "{} {}({params}) BWAPI_C_NOEXCEPT;{comment}",
        sig.ret, m.c_name
    )
    .unwrap();
}

fn constants<'a>(out: &mut String, scope: &str, consts: impl Iterator<Item = (&'a String, i32)>) {
    writeln!(out, "\nenum {{").unwrap();
    for (name, value) in consts {
        writeln!(out, "  {scope}_{name} = {value},").unwrap();
    }
    out.push_str("};\n");
}
