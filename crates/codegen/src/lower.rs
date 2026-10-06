//! Semantic rules of `bwapi.api`: syntax tree → IR.
//!
//! Only what neither the C++ compiler nor rustc catches is checked here: rules
//! that keep the generated API sound, and constructs the projections cannot emit.
//! Duplicate names, wrong C++ types or values fail later, when the output builds.
//! All errors are reported at once, each with the span of the offending token.

use std::collections::HashMap;

use proc_macro2::Span;
use syn::spanned::Spanned;

use crate::ir::*;
use crate::names;
use crate::parse::{self, Attr, Entry, SectionKind};

pub fn lower(file: &parse::File) -> syn::Result<Api> {
    let mut cx = Lowering::default();
    let api = cx.file(file);
    match cx.errors {
        Some(e) => Err(e),
        None => Ok(api),
    }
}

/// What a section name means when it is used as a marker type.
#[derive(Clone, Copy)]
enum Marker {
    Handle { has_set: bool },
    TypeId,
    Enum,
}

#[derive(Default)]
struct Lowering {
    errors: Option<syn::Error>,
    markers: HashMap<String, Marker>,
    /// C names taken so far: one C namespace for the whole manifest.
    c_names: HashMap<String, Span>,
}

/// Where a marker type occurs: the rules differ.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pos {
    Return,
    Arg,
}

impl Lowering {
    fn error(&mut self, span: Span, msg: impl std::fmt::Display) {
        let e = syn::Error::new(span, msg);
        match &mut self.errors {
            Some(all) => all.combine(e),
            None => self.errors = Some(e),
        }
    }

    fn file(&mut self, file: &parse::File) -> Api {
        for s in &file.sections {
            let marker = match s.kind {
                SectionKind::Handle => Marker::Handle {
                    has_set: s.attrs.iter().any(|a| a.name == "set"),
                },
                SectionKind::TypeId => Marker::TypeId,
                SectionKind::Enum => Marker::Enum,
                _ => continue,
            };
            self.markers.insert(s.name.to_string(), marker);
        }

        let mut api = Api::default();
        for s in &file.sections {
            let name = s.name.to_string();
            let cpp = s.cpp.value();
            let mut set = None;
            for a in &s.attrs {
                if a.name == "set" && s.kind == SectionKind::Handle {
                    set = Some(self.reason(a));
                } else {
                    self.error(
                        a.name.span(),
                        "only `handle` sections take an attribute: `#[set = \"C++ set class\"]`",
                    );
                }
            }
            match s.kind {
                SectionKind::Handle | SectionKind::TypeId => {
                    // A typeid receiver is an id: no match brand to give a returned handle.
                    let (methods, skips) = self.fns(s, s.kind == SectionKind::TypeId, &name);
                    let class = Class {
                        name: RustIdent(name),
                        cpp,
                        methods,
                        skips,
                    };
                    if s.kind == SectionKind::Handle {
                        api.handles.push(Handle { class, set });
                    } else {
                        api.type_ids.push(class);
                    }
                }
                SectionKind::Static => {
                    if !matches!(self.markers.get(&name), Some(Marker::TypeId)) {
                        self.error(
                            s.name.span(),
                            "a `static` section projects onto a `typeid` type",
                        );
                    }
                    let (functions, skips) = self.fns(s, true, names::c_scope(&cpp));
                    let consts = self.consts(s);
                    api.statics.push(Static {
                        owner: RustIdent(name),
                        cpp,
                        functions,
                        consts,
                        skips,
                    });
                }
                SectionKind::Enum => {
                    let variants = self.consts(s);
                    api.enums.push(Enum {
                        name: RustIdent(name),
                        cpp,
                        variants,
                    });
                }
            }
        }
        api
    }

    /// The reason of `#[attr = "reason"]`: it must say something.
    fn reason(&mut self, attr: &Attr) -> String {
        let v = attr.value.value();
        if v.trim().is_empty() {
            self.error(
                attr.value.span(),
                format!("`#[{}]` needs a non-empty reason", attr.name),
            );
        }
        v
    }

    fn attrs(&mut self, attrs: &[Attr], allowed: &[&str]) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for a in attrs {
            let name = a.name.to_string();
            if allowed.contains(&name.as_str()) {
                let reason = self.reason(a);
                out.insert(name, reason);
            } else {
                self.error(
                    a.name.span(),
                    format!("unknown attribute; expected one of: {}", allowed.join(", ")),
                );
            }
        }
        out
    }

    /// The C name of a function: `{c_prefix}_{cppName}`.
    fn fns(
        &mut self,
        s: &parse::Section,
        no_handles: bool,
        c_prefix: &str,
    ) -> (Vec<Method>, Vec<Skip>) {
        let mut methods = Vec::new();
        let mut skips = Vec::new();
        let mut overloads = HashMap::<String, usize>::new();
        for entry in &s.entries {
            if let Entry::Fn(f) = entry
                && f.sig.is_some()
            {
                *overloads.entry(f.cpp.to_string()).or_default() += 1;
            }
        }
        for entry in &s.entries {
            let f = match entry {
                Entry::Fn(f) => f,
                Entry::Const(c) if s.kind != SectionKind::Static => {
                    self.error(
                        c.name.span(),
                        "`const` is allowed only in `static` and `enum` sections",
                    );
                    continue;
                }
                Entry::Const(_) => continue,
            };
            let attrs = self.attrs(&f.attrs, &["skip", "non_null", "stable_address"]);
            let name = RustIdent(f.name.to_string());
            let cpp = CppIdent(f.cpp.to_string());
            let Some(sig) = &f.sig else {
                match attrs.get("skip") {
                    Some(reason) if attrs.len() == 1 => skips.push(Skip {
                        name,
                        cpp,
                        reason: reason.clone(),
                    }),
                    _ => self.error(f.name.span(), "a declaration without signature is `#[skip = \"reason\"] fn name = cppName;`"),
                }
                continue;
            };
            if attrs.contains_key("skip") {
                self.error(
                    f.name.span(),
                    "`#[skip]` goes on a declaration without signature",
                );
            }

            let mut args = Vec::new();
            // How many leading arguments a call cannot omit: the rest have C++ defaults.
            let mut required = None;
            for (i, a) in sig.args.iter().enumerate() {
                let Some(ty) = self.ty(&a.ty, Pos::Arg) else {
                    continue;
                };
                match (a.default, &ty) {
                    (Some(span), Type::Filter) => self.error(
                        span,
                        "a filter is never passed from Rust; `#[default]` marks an argument Rust may pass or omit",
                    ),
                    (Some(_), _) => {
                        required.get_or_insert(args.len());
                    }
                    (None, Type::Filter) => {}
                    (None, _) if required.is_some() => self.error(
                        a.name.span(),
                        "after a `#[default]` argument every argument has a default: a C++ call omits only a suffix",
                    ),
                    (None, _) => {}
                }
                if no_handles && contains_handle(&ty) {
                    self.error(
                        a.ty.span(),
                        "handles as arguments of `typeid` and `static` functions are not supported",
                    );
                }
                if ty == Type::Fmt && i + 1 != sig.args.len() {
                    self.error(a.ty.span(), "`Fmt` must be the last argument");
                }
                let is_filter = ty == Type::Filter;
                if is_filter != a.omitted.is_some() {
                    self.error(
                        a.ty.span(),
                        "filters cannot be passed from Rust: write `pred: UnitFilter = \"nullptr\"`; `= \"…\"` is allowed only for filters",
                    );
                }
                args.push(Arg {
                    name: RustIdent(a.name.to_string()),
                    ty,
                    omitted: a.omitted.as_ref().map(|lit| lit.value()),
                });
            }

            let ret = match &sig.ret {
                None => Type::Void,
                Some(ty) => {
                    let Some(t) = self.ty(ty, Pos::Return) else {
                        continue;
                    };
                    if no_handles && contains_handle(&t) {
                        self.error(
                            ty.span(),
                            "`typeid` and `static` functions cannot return a handle: there is no handle receiver to take the match brand from",
                        );
                    }
                    t
                }
            };
            let non_null = matches!(
                ret,
                Type::Handle {
                    nullability: Nullability::ProvenNonNull,
                    ..
                }
            );
            if non_null != attrs.contains_key("non_null") {
                self.error(
                    f.name.span(),
                    "`NonNull<T>` (a bwapi.api marker, not `std::ptr::NonNull`) and `#[non_null = \"why BWAPI never returns null\"]` go together",
                );
            }
            if matches!(ret, Type::Set(_)) != attrs.contains_key("stable_address") {
                self.error(
                    f.name.span(),
                    "`Set<T>` and `#[stable_address = \"why the set stays at one address for the whole match\"]` go together: `const XSet&` alone does not prove it",
                );
            }
            // An overload is named in C by the types of the arguments a call cannot
            // omit: `Unit_attack_Position`, `Unit_canBuild_UnitType_TilePosition`.
            let mut c_name = format!("{c_prefix}_{}", cpp.0);
            if overloads[&cpp.0] > 1 {
                let end = required.unwrap_or(args.len());
                for a in args[..end].iter().filter(|a| a.omitted.is_none()) {
                    c_name.push('_');
                    c_name.push_str(&c_type_name(&a.ty));
                }
            }
            // Defaults: `name` omits them and lets C++ apply its own, `name_with` passes all.
            // In C the full call is `c_name`, the short one `c_name_d`.
            if let Some(required) = required {
                self.c_name(format!("{c_name}_d"), f.name.span());
                self.c_name(c_name.clone(), f.name.span());
                methods.push(Method {
                    name: name.clone(),
                    c_name: format!("{c_name}_d"),
                    cpp: cpp.clone(),
                    args: args[..required].to_vec(),
                    ret: ret.clone(),
                });
                methods.push(Method {
                    name: RustIdent(format!("{}_with", name.0)),
                    c_name,
                    cpp,
                    args,
                    ret,
                });
            } else {
                self.c_name(c_name.clone(), f.name.span());
                methods.push(Method {
                    name,
                    c_name,
                    cpp,
                    args,
                    ret,
                });
            }
        }
        (methods, skips)
    }

    fn c_name(&mut self, name: String, span: Span) {
        if self.c_names.insert(name.clone(), span).is_some() {
            self.error(
                span,
                format!("the C function `{name}` is already generated for another declaration"),
            );
        }
    }

    /// `static`: `const CppName = v;`, the Rust name is SCREAMING_SNAKE.
    /// `enum`: `const RustName = CppName = v;`, an UpperCamelCase variant with a unique value.
    fn consts(&mut self, s: &parse::Section) -> Vec<Const> {
        let is_enum = s.kind == SectionKind::Enum;
        let mut consts: Vec<Const> = Vec::new();
        for entry in &s.entries {
            let c = match entry {
                Entry::Const(c) => c,
                Entry::Fn(f) if is_enum => {
                    self.error(
                        f.fn_span,
                        "an `enum` section holds only `const` declarations",
                    );
                    continue;
                }
                Entry::Fn(_) => continue,
            };
            let value = match c.value.base10_parse::<i32>() {
                Ok(v) => v,
                Err(e) => {
                    self.error(c.value.span(), e);
                    continue;
                }
            };
            let (rust, cpp) = match (is_enum, &c.cpp) {
                (false, None) => (
                    names::screaming_snake(&c.name.to_string()),
                    c.name.to_string(),
                ),
                (true, Some(cpp)) if names::is_upper_camel(&c.name.to_string()) => {
                    (c.name.to_string(), cpp.to_string())
                }
                (false, Some(_)) => {
                    self.error(
                        c.name.span(),
                        "`static` constants are named mechanically: `const CppName = value;`",
                    );
                    continue;
                }
                (true, _) => {
                    self.error(
                        c.name.span(),
                        "an enum variant is `const UpperCamelName = CppName = value;`",
                    );
                    continue;
                }
            };
            if let Some(other) = consts.iter().find(|o| is_enum && o.value == value) {
                self.error(
                    c.value.span(),
                    format!("value {value} is already taken by `{}`", other.rust.0),
                );
            }
            consts.push(Const {
                cpp: CppIdent(cpp),
                rust: RustIdent(rust),
                value,
            });
        }
        consts
    }

    /// A marker type and the rules of its position.
    fn ty(&mut self, ty: &syn::Type, pos: Pos) -> Option<Type> {
        let t = self.marker(ty)?;
        let ok = match (&t, pos) {
            (Type::Handle { nullability, .. }, Pos::Return) => {
                *nullability != Nullability::Required
            }
            (Type::Handle { nullability, .. }, Pos::Arg) => {
                *nullability != Nullability::ProvenNonNull
            }
            (Type::Vec(e), Pos::Return) => vec_elem_ok(e),
            (Type::Pair(a, b), Pos::Return) => pair_elem_ok(a) && pair_elem_ok(b),
            (Type::Set(_) | Type::Query(_), Pos::Return) => true,
            (Type::Vec(_) | Type::Pair(..) | Type::Set(_) | Type::Query(_), Pos::Arg) => false,
            // A `const char*` may be null; an enum from the game may lie outside the Rust enum.
            (Type::CStr | Type::Fmt | Type::Filter | Type::Enum(_), Pos::Return) => false,
            _ => true,
        };
        if !ok {
            let msg = match (&t, pos) {
                (Type::Handle { name, .. }, Pos::Return) => {
                    format!("a handle return is `Option<{name}>` or `NonNull<{name}>`")
                }
                (_, Pos::Return) => {
                    "not allowed in a return; see the type table in bwapi.api".to_owned()
                }
                (_, Pos::Arg) => {
                    "not allowed as an argument; see the type table in bwapi.api".to_owned()
                }
            };
            self.error(ty.span(), msg);
        }
        Some(t)
    }

    /// Maps a syntactic type to a marker; reports unknown markers.
    fn marker(&mut self, ty: &syn::Type) -> Option<Type> {
        let unknown = |this: &mut Self| {
            this.error(ty.span(), "unknown marker type");
            None
        };
        if let syn::Type::Tuple(t) = ty {
            if t.elems.len() != 2 {
                return unknown(self);
            }
            let a = self.marker(&t.elems[0])?;
            let b = self.marker(&t.elems[1])?;
            return Some(Type::Pair(Box::new(a), Box::new(b)));
        }
        let syn::Type::Path(p) = ty else {
            return unknown(self);
        };
        if p.qself.is_some() || p.path.segments.len() != 1 {
            return unknown(self);
        }
        let seg = &p.path.segments[0];
        let name = seg.ident.to_string();
        let inner = match &seg.arguments {
            syn::PathArguments::None => None,
            syn::PathArguments::AngleBracketed(a) if a.args.len() == 1 => match &a.args[0] {
                syn::GenericArgument::Type(t) => Some(t),
                _ => return unknown(self),
            },
            _ => return unknown(self),
        };
        let Some(inner) = inner else {
            return Some(match name.as_str() {
                "bool" => Type::Bool,
                "i32" => Type::I32,
                "u32" => Type::U32,
                "f64" => Type::F64,
                "CStr" => Type::CStr,
                "CxxString" => Type::CxxString,
                "Fmt" => Type::Fmt,
                "UnitFilter" | "BestUnitFilter" => Type::Filter,
                "Position" => Type::Value(ValueKind::Position),
                "TilePosition" => Type::Value(ValueKind::TilePosition),
                "WalkPosition" => Type::Value(ValueKind::WalkPosition),
                _ => match self.markers.get(&name) {
                    Some(Marker::Handle { .. }) => Type::Handle {
                        name,
                        nullability: Nullability::Required,
                    },
                    Some(Marker::TypeId) => Type::TypeId(name),
                    Some(Marker::Enum) => Type::Enum(name),
                    None => return unknown(self),
                },
            });
        };
        let inner_ty = self.marker(inner)?;
        if name == "Vec" {
            return Some(Type::Vec(Box::new(inner_ty)));
        }
        let Type::Handle {
            name: h,
            nullability: Nullability::Required,
        } = inner_ty
        else {
            self.error(inner.span(), format!("`{name}<T>` takes a handle type"));
            return None;
        };
        let has_set = matches!(self.markers.get(&h), Some(Marker::Handle { has_set: true }));
        Some(match name.as_str() {
            "Option" => Type::Handle {
                name: h,
                nullability: Nullability::Nullable,
            },
            "NonNull" => Type::Handle {
                name: h,
                nullability: Nullability::ProvenNonNull,
            },
            "Set" | "Query" if !has_set => {
                self.error(
                    ty.span(),
                    format!("`{name}<{h}>` needs `#[set = \"C++ set class\"]` on `handle {h}`"),
                );
                return None;
            }
            "Set" => Type::Set(h),
            "Query" => Type::Query(h),
            _ => return unknown(self),
        })
    }
}

/// An argument type in the C name of an overload.
fn c_type_name(t: &Type) -> String {
    match t {
        Type::Bool => "bool".into(),
        Type::I32 => "int".into(),
        Type::U32 => "unsigned".into(),
        Type::F64 => "double".into(),
        Type::Handle { name, .. } | Type::TypeId(name) | Type::Enum(name) => name.clone(),
        Type::Value(v) => v.rust().into(),
        Type::CStr | Type::CxxString | Type::Fmt => "string".into(),
        _ => unreachable!("lowering restricts argument markers"),
    }
}

fn pair_elem_ok(t: &Type) -> bool {
    matches!(t, Type::I32 | Type::TypeId(_))
}

fn vec_elem_ok(t: &Type) -> bool {
    match t {
        Type::I32 | Type::TypeId(_) | Type::Value(_) => true,
        Type::Handle { nullability, .. } => *nullability == Nullability::Required,
        Type::Pair(a, b) => pair_elem_ok(a) && pair_elem_ok(b),
        _ => false,
    }
}

/// Recursively: does the type mention a handle anywhere?
fn contains_handle(t: &Type) -> bool {
    match t {
        Type::Handle { .. } | Type::Set(_) | Type::Query(_) => true,
        Type::Vec(e) => contains_handle(e),
        Type::Pair(a, b) => contains_handle(a) || contains_handle(b),
        _ => false,
    }
}
