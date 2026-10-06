//! IR of the manifest: what both projections are generated from.
//!
//! It describes the manifest, not C++: every C++ fact the projections need is
//! either a name written in `bwapi.api` or follows from a marker type.

/// Name of a Rust item.
#[derive(Clone)]
pub struct RustIdent(pub String);

/// Name of a C++ entity; may be a Rust keyword (`self`, `move`).
#[derive(Clone)]
pub struct CppIdent(pub String);

#[derive(Default)]
pub struct Api {
    pub handles: Vec<Handle>,
    pub type_ids: Vec<Class>,
    pub statics: Vec<Static>,
    pub enums: Vec<Enum>,
}

impl Api {
    pub fn handle(&self, name: &str) -> &Handle {
        self.handles
            .iter()
            .find(|h| h.class.name.0 == name)
            .expect("lowering resolved the name")
    }

    /// The C++ set holding the handle: `BWAPI::Unitset`.
    pub fn set_class(&self, handle: &str) -> &str {
        self.handle(handle)
            .set
            .as_deref()
            .expect("lowering checked #[set]")
    }

    pub fn type_id(&self, name: &str) -> &Class {
        self.type_ids
            .iter()
            .find(|t| t.name.0 == name)
            .expect("lowering resolved the name")
    }

    pub fn enum_(&self, name: &str) -> &Enum {
        self.enums
            .iter()
            .find(|e| e.name.0 == name)
            .expect("lowering resolved the name")
    }
}

/// A `handle` or `typeid` section. `cpp` is the qualified class: `BWAPI::UnitInterface`.
pub struct Class {
    pub name: RustIdent,
    pub cpp: String,
    pub methods: Vec<Method>,
    pub skips: Vec<Skip>,
}

pub struct Handle {
    pub class: Class,
    /// The C++ set holding this handle: `BWAPI::Unitset`.
    pub set: Option<String>,
}

/// Functions and constants of a C++ namespace, projected onto a typeid.
pub struct Static {
    pub owner: RustIdent,
    pub cpp: String,
    pub functions: Vec<Method>,
    pub consts: Vec<Const>,
    pub skips: Vec<Skip>,
}

/// A C++ enum whose values come from the user: a Rust `#[repr(i32)] enum`.
pub struct Enum {
    pub name: RustIdent,
    pub cpp: String,
    pub variants: Vec<Const>,
}

pub struct Const {
    pub cpp: CppIdent,
    pub rust: RustIdent,
    pub value: i32,
}

pub struct Method {
    pub name: RustIdent,
    /// The C function: `Unit_attack_Position`, `Unit_attack_Position_d`.
    pub c_name: String,
    pub cpp: CppIdent,
    pub args: Vec<Arg>,
    pub ret: Type,
}

pub struct Skip {
    pub name: RustIdent,
    pub cpp: CppIdent,
    pub reason: String,
}

#[derive(Clone)]
pub struct Arg {
    pub name: RustIdent,
    pub ty: Type,
    /// Opaque C++ expression passed instead of a Rust argument.
    pub omitted: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nullability {
    /// A handle argument.
    Required,
    /// `Option<H>`.
    Nullable,
    /// `NonNull<H>` with `#[non_null = "…"]`: null is a BWAPI contract violation.
    ProvenNonNull,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Position,
    TilePosition,
    WalkPosition,
}

impl ValueKind {
    pub fn rust(self) -> &'static str {
        match self {
            ValueKind::Position => "Position",
            ValueKind::TilePosition => "TilePosition",
            ValueKind::WalkPosition => "WalkPosition",
        }
    }

    pub fn cpp(self) -> &'static str {
        match self {
            ValueKind::Position => "BWAPI::Position",
            ValueKind::TilePosition => "BWAPI::TilePosition",
            ValueKind::WalkPosition => "BWAPI::WalkPosition",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum Type {
    Void,
    Bool,
    I32,
    U32,
    F64,
    Handle {
        name: String,
        nullability: Nullability,
    },
    Value(ValueKind),
    TypeId(String),
    Enum(String),
    CStr,
    CxxString,
    Fmt,
    /// `UnitFilter` or `BestUnitFilter`: always omitted.
    Filter,
    Vec(Box<Type>),
    Pair(Box<Type>, Box<Type>),
    /// A live set: BWAPI returns `const XSet&` at a stable address.
    Set(String),
    /// A set returned by value, owned by the C++ heap.
    Query(String),
}
