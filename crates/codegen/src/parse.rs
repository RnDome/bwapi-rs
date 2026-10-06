//! Syntax of `bwapi.api`: a Rust-shaped DSL parsed with `syn`.
//!
//! ```text
//! file    := 'bwapi_api' '!' '{' section* '}'
//! section := attr* kind RustIdent '=' LitStr '{' entry* '}'
//! kind    := 'handle' | 'game' | 'typeid' | 'static' | 'enum'
//! entry   := attr* (method | skip | const)
//! method  := 'fn' RustIdent '(' [arg (',' arg)* [',']] ')' ['->' Type] '=' CppIdent ';'
//! skip    := 'fn' RustIdent '=' CppIdent ';'
//! arg     := RustIdent ':' Type ['=' LitStr]
//! const   := 'const' CppIdent '=' LitInt ';'                  # static: Rust name is SCREAMING_SNAKE
//!          | 'const' RustIdent '=' CppIdent '=' LitInt ';'   # enum: explicit UpperCamelCase variant
//! attr    := '#' '[' Ident '=' LitStr ']'
//! ```
//!
//! This module only recognizes the grammar; every semantic rule lives in `lower`.

use proc_macro2::Span;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, LitInt, LitStr, Token, Type, braced, parenthesized};

pub struct File {
    pub sections: Vec<Section>,
}

pub struct Section {
    pub attrs: Vec<Attr>,
    pub kind: SectionKind,
    pub name: Ident,
    pub cpp: LitStr,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SectionKind {
    Handle,
    TypeId,
    Static,
    Enum,
}

/// `#[name = "value"]`.
pub struct Attr {
    pub name: Ident,
    pub value: LitStr,
}

pub enum Entry {
    Fn(Box<FnDecl>),
    Const(ConstDecl),
}

pub struct FnDecl {
    pub attrs: Vec<Attr>,
    pub fn_span: Span,
    pub name: Ident,
    /// `None` for a `skip` declaration: `fn name = cppName;`.
    pub sig: Option<Signature>,
    /// Parsed with `Ident::parse_any`: C++ names may be Rust keywords (`self`, `move`).
    pub cpp: Ident,
}

pub struct Signature {
    pub args: Vec<ArgDecl>,
    pub ret: Option<Type>,
}

pub struct ArgDecl {
    /// `#[default]`: the C++ parameter has a default argument.
    pub default: Option<Span>,
    pub name: Ident,
    pub ty: Type,
    /// Opaque C++ expression passed in place of the argument.
    pub omitted: Option<LitStr>,
}

/// `const CppName = 0;` or, with an explicit Rust name, `const RustName = CppName = 0;`.
pub struct ConstDecl {
    pub attrs: Vec<Attr>,
    /// The Rust name when `cpp` is present, the C++ name otherwise.
    pub name: Ident,
    pub cpp: Option<Ident>,
    pub value: LitInt,
}

mod kw {
    syn::custom_keyword!(bwapi_api);
    syn::custom_keyword!(handle);
    syn::custom_keyword!(typeid);
}

impl Parse for File {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::bwapi_api>()?;
        input.parse::<Token![!]>()?;
        let body;
        braced!(body in input);
        let mut sections = Vec::new();
        while !body.is_empty() {
            sections.push(body.parse()?);
        }
        Ok(File { sections })
    }
}

fn parse_attrs(input: ParseStream) -> syn::Result<Vec<Attr>> {
    let mut attrs = Vec::new();
    while input.peek(Token![#]) {
        input.parse::<Token![#]>()?;
        let inner;
        syn::bracketed!(inner in input);
        let name: Ident = inner.parse()?;
        inner.parse::<Token![=]>()?;
        let value: LitStr = inner.parse()?;
        if !inner.is_empty() {
            return Err(inner.error("expected `#[name = \"value\"]`"));
        }
        attrs.push(Attr { name, value });
    }
    Ok(attrs)
}

impl Parse for Section {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = parse_attrs(input)?;
        let lookahead = input.lookahead1();
        let kind = if lookahead.peek(kw::handle) {
            input.parse::<kw::handle>()?;
            SectionKind::Handle
        } else if lookahead.peek(kw::typeid) {
            input.parse::<kw::typeid>()?;
            SectionKind::TypeId
        } else if lookahead.peek(Token![static]) {
            input.parse::<Token![static]>()?;
            SectionKind::Static
        } else if lookahead.peek(Token![enum]) {
            input.parse::<Token![enum]>()?;
            SectionKind::Enum
        } else {
            return Err(lookahead.error());
        };
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let cpp: LitStr = input.parse()?;
        let body;
        braced!(body in input);
        let mut entries = Vec::new();
        while !body.is_empty() {
            entries.push(body.parse()?);
        }
        Ok(Section {
            attrs,
            kind,
            name,
            cpp,
            entries,
        })
    }
}

impl Parse for Entry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = parse_attrs(input)?;
        let lookahead = input.lookahead1();
        if lookahead.peek(Token![fn]) {
            let fn_span = input.parse::<Token![fn]>()?.span;
            let name: Ident = input.parse()?;
            let sig = if input.peek(syn::token::Paren) {
                let inner;
                parenthesized!(inner in input);
                let args = Punctuated::<ArgDecl, Token![,]>::parse_terminated(&inner)?;
                let ret = if input.peek(Token![->]) {
                    input.parse::<Token![->]>()?;
                    Some(input.parse()?)
                } else {
                    None
                };
                Some(Signature {
                    args: args.into_iter().collect(),
                    ret,
                })
            } else {
                None
            };
            input.parse::<Token![=]>()?;
            let cpp = Ident::parse_any(input)?;
            input.parse::<Token![;]>()?;
            Ok(Entry::Fn(Box::new(FnDecl {
                attrs,
                fn_span,
                name,
                sig,
                cpp,
            })))
        } else if lookahead.peek(Token![const]) {
            input.parse::<Token![const]>()?;
            let name = Ident::parse_any(input)?;
            input.parse::<Token![=]>()?;
            let cpp = if input.peek(LitInt) {
                None
            } else {
                let cpp = Ident::parse_any(input)?;
                input.parse::<Token![=]>()?;
                Some(cpp)
            };
            let value: LitInt = input.parse()?;
            input.parse::<Token![;]>()?;
            Ok(Entry::Const(ConstDecl {
                attrs,
                name,
                cpp,
                value,
            }))
        } else {
            Err(lookahead.error())
        }
    }
}

impl Parse for ArgDecl {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let default = if input.peek(Token![#]) {
            input.parse::<Token![#]>()?;
            let inner;
            syn::bracketed!(inner in input);
            let attr: Ident = inner.parse()?;
            if attr != "default" || !inner.is_empty() {
                return Err(syn::Error::new(
                    attr.span(),
                    "an argument takes only `#[default]`",
                ));
            }
            Some(attr.span())
        } else {
            None
        };
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        let omitted = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(ArgDecl {
            default,
            name,
            ty,
            omitted,
        })
    }
}
