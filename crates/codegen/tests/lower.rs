//! Rules of `bwapi.api` lowering: every rule rejects what it must, at the token it
//! must point to, and accepts what it must.

use codegen::ir::{Nullability, Type};
use codegen::lower_str;
use proc_macro2::Span;

/// Sections every test can refer to.
const BASE: &str = r#"
    #[set = "BWAPI::Unitset"]
    handle Unit = "BWAPI::UnitInterface" {}
    handle Player = "BWAPI::PlayerInterface" {}
    typeid UnitType = "BWAPI::UnitType" {}
    enum Key = "BWAPI::Key" { const A = K_A = 65; }
"#;

fn manifest(sections: &str) -> String {
    format!("bwapi_api! {{{BASE}{sections}}}")
}

/// A `Game` handle with these declarations.
fn game(decls: &str) -> String {
    manifest(&format!(r#"handle Game = "BWAPI::Game" {{ {decls} }}"#))
}

/// The source text a span covers.
fn text(src: &str, span: Span) -> String {
    let (start, end) = (span.start(), span.end());
    let line = src
        .lines()
        .nth(start.line - 1)
        .expect("span inside the source");
    let len = if end.line == start.line {
        end.column - start.column
    } else {
        usize::MAX
    };
    line.chars().skip(start.column).take(len).collect()
}

/// Asserts that lowering fails with exactly these errors, in order: the text each
/// error points to and a fragment of its message.
#[track_caller]
fn rejects(src: &str, expected: &[(&str, &str)]) {
    let errors = match lower_str(src) {
        Ok(_) => panic!("accepted:\n{src}"),
        Err(e) => e,
    };
    let got: Vec<(String, String)> = errors
        .into_iter()
        .map(|e| (text(src, e.span()), e.to_string()))
        .collect();
    assert_eq!(
        got.len(),
        expected.len(),
        "expected {expected:?}, got {got:?}"
    );
    for ((at, msg), (want_at, want_msg)) in got.iter().zip(expected) {
        assert_eq!(at, want_at, "error `{msg}` points elsewhere");
        assert!(
            msg.contains(want_msg),
            "error at `{at}`: `{msg}` does not contain `{want_msg}`"
        );
    }
}

#[track_caller]
fn accepts(src: &str) -> codegen::ir::Api {
    match lower_str(src) {
        Ok(api) => api,
        Err(e) => panic!("rejected: {}", codegen::render_errors("test.api", &e)),
    }
}

// ------------------------------------------------------------------ the manifest

#[test]
fn bwapi_api_is_accepted() {
    accepts(include_str!("../../../bwapi.api"));
}

#[test]
fn unknown_section_kind() {
    rejects(
        &manifest(r#"game Game = "BWAPI::Game" {}"#),
        &[("game", "expected one of")],
    );
}

#[test]
fn all_errors_are_reported_at_once() {
    rejects(
        &game(
            "fn a() -> Unit = getA;
             fn b() -> String = getB;",
        ),
        &[("Unit", "handle return"), ("String", "unknown marker")],
    );
}

// ------------------------------------------------------------------ markers

#[test]
fn unknown_marker() {
    rejects(
        &game("fn name() -> String = getName;"),
        &[("String", "unknown marker type")],
    );
}

#[test]
fn unknown_generic_marker() {
    rejects(
        &game("fn units() -> Box<Unit> = getUnits;"),
        &[("Box<Unit>", "unknown marker type")],
    );
}

#[test]
fn tuple_of_three() {
    rejects(
        &game("fn t() -> (i32, i32, i32) = getT;"),
        &[("(i32, i32, i32)", "unknown marker type")],
    );
}

#[test]
fn handle_wrapper_takes_a_plain_handle() {
    rejects(
        &game(
            "fn a() -> Option<i32> = getA;
             fn b() -> Option<Option<Unit>> = getB;",
        ),
        &[
            ("i32", "`Option<T>` takes a handle type"),
            ("Option<Unit>", "`Option<T>` takes a handle type"),
        ],
    );
}

// ------------------------------------------------------------------ handles

#[test]
fn bare_handle_return() {
    rejects(
        &game("fn target() -> Unit = getTarget;"),
        &[(
            "Unit",
            "a handle return is `Option<Unit>` or `NonNull<Unit>`",
        )],
    );
}

#[test]
fn non_null_without_attribute() {
    rejects(
        &game("fn player() -> NonNull<Player> = getPlayer;"),
        &[("player", "`#[non_null")],
    );
}

#[test]
fn non_null_attribute_without_non_null() {
    rejects(
        &game(
            r#"#[non_null = "never null"]
            fn player() -> Option<Player> = getPlayer;"#,
        ),
        &[("player", "`#[non_null")],
    );
}

#[test]
fn non_null_argument() {
    rejects(
        &game("fn f(unit: NonNull<Unit>) = f;"),
        &[("NonNull<Unit>", "not allowed as an argument")],
    );
}

#[test]
fn handle_nullability() {
    let api = accepts(&game(
        r#"fn a(unit: Unit, player: Option<Player>) -> Option<Unit> = getA;
        #[non_null = "never null"]
        fn b() -> NonNull<Player> = getB;"#,
    ));
    let game = api.handle("Game");
    let a = &game.class.methods[0];
    assert!(matches!(
        a.args[0].ty,
        Type::Handle {
            nullability: Nullability::Required,
            ..
        }
    ));
    assert!(matches!(
        a.args[1].ty,
        Type::Handle {
            nullability: Nullability::Nullable,
            ..
        }
    ));
    assert!(matches!(
        a.ret,
        Type::Handle {
            nullability: Nullability::Nullable,
            ..
        }
    ));
    assert!(matches!(
        game.class.methods[1].ret,
        Type::Handle {
            nullability: Nullability::ProvenNonNull,
            ..
        }
    ));
}

// ------------------------------------------------------------------ sets

#[test]
fn set_without_stable_address() {
    rejects(
        &game("fn all_units() -> Set<Unit> = getAllUnits;"),
        &[("all_units", "`#[stable_address")],
    );
}

#[test]
fn stable_address_without_set() {
    rejects(
        &game(
            r#"#[stable_address = "a member"]
            fn units() -> Query<Unit> = getUnits;"#,
        ),
        &[("units", "`#[stable_address")],
    );
}

#[test]
fn set_of_handle_without_set_class() {
    rejects(
        &game(
            r#"#[stable_address = "a member"]
            fn players() -> Set<Player> = getPlayers;
            fn allies() -> Query<Player> = allies;"#,
        ),
        &[
            (
                "Set<Player>",
                "needs `#[set = \"C++ set class\"]` on `handle Player`",
            ),
            (
                "Query<Player>",
                "needs `#[set = \"C++ set class\"]` on `handle Player`",
            ),
        ],
    );
}

#[test]
fn set_of_non_handle() {
    rejects(
        &game("fn types() -> Query<UnitType> = getTypes;"),
        &[("UnitType", "`Query<T>` takes a handle type")],
    );
}

#[test]
fn set_class_only_on_handles() {
    rejects(
        &manifest(r#"#[set = "BWAPI::UnitType::set"] typeid TechType = "BWAPI::TechType" {}"#),
        &[("set", "only `handle` sections take an attribute")],
    );
}

#[test]
fn sets_are_accepted() {
    let api = accepts(&game(
        r#"#[stable_address = "a member"]
        fn all_units() -> Set<Unit> = getAllUnits;
        fn units_in_radius(radius: i32) -> Query<Unit> = getUnitsInRadius;"#,
    ));
    assert_eq!(api.handle("Unit").set.as_deref(), Some("BWAPI::Unitset"));
    let methods = &api.handle("Game").class.methods;
    assert!(methods[0].ret == Type::Set("Unit".into()));
    assert!(methods[1].ret == Type::Query("Unit".into()));
}

// ------------------------------------------------------------------ positions

#[test]
fn containers_only_in_returns() {
    rejects(
        &game(
            r#"fn a(units: Query<Unit>) = a;
            fn b(types: Vec<UnitType>) = b;
            fn c(pair: (i32, i32)) = c;"#,
        ),
        &[
            ("Query<Unit>", "not allowed as an argument"),
            ("Vec<UnitType>", "not allowed as an argument"),
            ("(i32, i32)", "not allowed as an argument"),
        ],
    );
}

#[test]
fn arguments_only_markers_in_returns() {
    rejects(
        &game(
            r#"fn a() -> CStr = a;
            fn b() -> Fmt = b;
            fn c() -> UnitFilter = c;
            fn d() -> Key = d;"#,
        ),
        &[
            ("CStr", "not allowed in a return"),
            ("Fmt", "not allowed in a return"),
            ("UnitFilter", "not allowed in a return"),
            ("Key", "not allowed in a return"),
        ],
    );
}

#[test]
fn vec_elements() {
    rejects(
        &game(
            r#"fn a() -> Vec<CxxString> = a;
            fn b() -> Vec<Option<Unit>> = b;
            fn c() -> Vec<Vec<i32>> = c;
            fn d() -> Vec<bool> = d;"#,
        ),
        &[
            ("Vec<CxxString>", "not allowed in a return"),
            ("Vec<Option<Unit>>", "not allowed in a return"),
            ("Vec<Vec<i32>>", "not allowed in a return"),
            ("Vec<bool>", "not allowed in a return"),
        ],
    );
}

#[test]
fn pair_elements() {
    rejects(
        &game(
            r#"fn a() -> (Position, i32) = a;
            fn b() -> Vec<(UnitType, f64)> = b;"#,
        ),
        &[
            ("(Position, i32)", "not allowed in a return"),
            ("Vec<(UnitType, f64)>", "not allowed in a return"),
        ],
    );
}

#[test]
fn containers_are_accepted() {
    let api = accepts(&game(
        r#"fn a() -> Vec<Unit> = a;
        fn b() -> Vec<Position> = b;
        fn c() -> Vec<(UnitType, i32)> = c;
        fn d() -> (UnitType, i32) = d;
        fn e() -> CxxString = e;"#,
    ));
    let methods = &api.handle("Game").class.methods;
    assert!(
        methods[2].ret
            == Type::Vec(Box::new(Type::Pair(
                Box::new(Type::TypeId("UnitType".into())),
                Box::new(Type::I32),
            )))
    );
}

// ------------------------------------------------------------------ arguments

#[test]
fn fmt_must_be_last() {
    rejects(
        &game("fn draw(text: Fmt, x: i32) = draw;"),
        &[("Fmt", "`Fmt` must be the last argument")],
    );
}

#[test]
fn filter_must_be_omitted() {
    rejects(
        &game("fn units(pred: UnitFilter) -> Query<Unit> = getUnits;"),
        &[("UnitFilter", "filters cannot be passed from Rust")],
    );
}

#[test]
fn only_filters_are_omitted() {
    rejects(
        &game(r#"fn units(radius: i32 = "123") -> Query<Unit> = getUnits;"#),
        &[("i32", "`= \"…\"` is allowed only for filters")],
    );
}

#[test]
fn omitted_filters_are_kept_in_order() {
    let api = accepts(&game(
        r#"fn closest(pred: UnitFilter = "nullptr", radius: i32) -> Option<Unit> = getClosestUnit;
        fn best(best: BestUnitFilter = "nullptr", pred: UnitFilter = "nullptr") -> Option<Unit> = getBestUnit;"#,
    ));
    let methods = &api.handle("Game").class.methods;
    let closest = &methods[0];
    assert!(closest.args[0].ty == Type::Filter);
    assert_eq!(closest.args[0].omitted.as_deref(), Some("nullptr"));
    assert!(closest.args[1].ty == Type::I32);
    assert_eq!(closest.args[1].omitted, None);
    assert!(methods[1].args.iter().all(|a| a.ty == Type::Filter));
}

#[test]
fn strings_and_values_as_arguments() {
    accepts(&game(
        "fn f(a: CStr, b: CxxString, c: Position, d: TilePosition, e: WalkPosition, f: Key, g: UnitType, h: u32, i: f64, j: bool, k: Fmt) = f;",
    ));
}

// ------------------------------------------------------------------ defaults

#[test]
fn defaults_give_short_and_full_methods() {
    let api = accepts(&game(
        r#"fn closest_unit(center: Position, pred: UnitFilter = "nullptr", #[default] radius: i32) -> Option<Unit> = getClosestUnit;
        fn stop(#[default] shift_queue_command: bool) -> bool = stop;"#,
    ));
    let methods: Vec<(&str, &str, Vec<&str>)> = api
        .handle("Game")
        .class
        .methods
        .iter()
        .map(|m| {
            let args = m.args.iter().map(|a| a.name.0.as_str()).collect();
            (m.name.0.as_str(), m.cpp.0.as_str(), args)
        })
        .collect();
    assert_eq!(
        methods,
        [
            ("closest_unit", "getClosestUnit", vec!["center", "pred"]),
            (
                "closest_unit_with",
                "getClosestUnit",
                vec!["center", "pred", "radius"]
            ),
            ("stop", "stop", vec![]),
            ("stop_with", "stop", vec!["shift_queue_command"]),
        ]
    );
}

#[test]
fn defaults_are_a_suffix() {
    rejects(
        &game("fn f(#[default] a: bool, b: i32, #[default] c: bool) = f;"),
        &[("b", "a C++ call omits only a suffix")],
    );
}

#[test]
fn filters_take_no_default() {
    rejects(
        &game(r#"fn f(#[default] pred: UnitFilter = "nullptr") = f;"#),
        &[("default", "a filter is never passed from Rust")],
    );
}

#[test]
fn arguments_take_only_default() {
    rejects(
        &game(r#"fn f(#[skip = "x"] a: bool) = f;"#),
        &[("skip", "an argument takes only `#[default]`")],
    );
}

// ------------------------------------------------------------------ C names

fn c_names(api: &codegen::ir::Api, handle: &str) -> Vec<String> {
    api.handle(handle)
        .class
        .methods
        .iter()
        .map(|m| m.c_name.clone())
        .collect()
}

#[test]
fn c_names_of_overloads() {
    let api = accepts(&game(
        r#"fn attack_position(target: Position, #[default] shift_queue_command: bool) -> bool = attack;
        fn attack_unit(target: Unit, #[default] shift_queue_command: bool) -> bool = attack;
        fn can_build(#[default] check: bool) -> bool = canBuild;
        fn can_build_at(unit_type: UnitType, target: TilePosition, #[default] check: bool) -> bool = canBuild;
        fn units_in_radius(center: Position, radius: i32, pred: UnitFilter = "nullptr") -> Query<Unit> = getUnitsInRadius;
        fn is_walkable(position: WalkPosition) -> bool = isWalkable;
        #[skip = "the same with integers"]
        fn is_walkable_xy = isWalkable;"#,
    ));
    assert_eq!(
        c_names(&api, "Game"),
        [
            "Game_attack_Position_d",
            "Game_attack_Position",
            "Game_attack_Unit_d",
            "Game_attack_Unit",
            "Game_canBuild_d",
            "Game_canBuild",
            "Game_canBuild_UnitType_TilePosition_d",
            "Game_canBuild_UnitType_TilePosition",
            "Game_getUnitsInRadius",
            "Game_isWalkable",
        ]
    );
}

#[test]
fn c_names_of_typeids_and_statics() {
    let api = accepts(&manifest(
        r#"typeid Race = "BWAPI::Race" { fn name() -> CxxString = getName; }
        static Race = "BWAPI::Races" { fn all() -> Vec<Race> = allRaces; }
        static Race = "BWAPI::Races::Enum" { const Zerg = 0; }"#,
    ));
    assert_eq!(api.type_id("Race").methods[0].c_name, "Race_getName");
    assert_eq!(api.statics[0].functions[0].c_name, "Races_allRaces");
}

#[test]
fn c_names_are_unique() {
    rejects(
        &game("fn f(x: i32) = f; fn g(y: i32) = f;"),
        &[("g", "the C function `Game_f_int` is already generated")],
    );
}

// ------------------------------------------------------------------ typeid and static

#[test]
fn typeid_cannot_return_handles() {
    rejects(
        &manifest(
            r#"typeid TechType = "BWAPI::TechType" { fn unit() -> Option<Unit> = getUnit; }"#,
        ),
        &[("Option<Unit>", "cannot return a handle")],
    );
}

#[test]
fn static_cannot_return_handles_anywhere_inside() {
    rejects(
        &manifest(
            r#"static UnitType = "BWAPI::UnitTypes" {
                fn a() -> Vec<Unit> = a;
                fn b() -> Query<Unit> = b;
            }"#,
        ),
        &[
            ("Vec<Unit>", "cannot return a handle"),
            ("Query<Unit>", "cannot return a handle"),
        ],
    );
}

#[test]
fn typeid_and_static_take_no_handles() {
    rejects(
        &manifest(
            r#"typeid TechType = "BWAPI::TechType" { fn f(unit: Unit) -> bool = f; }
            static UnitType = "BWAPI::UnitTypes" { fn g(player: Option<Player>) -> i32 = g; }"#,
        ),
        &[
            ("Unit", "handles as arguments of `typeid` and `static`"),
            (
                "Option<Player>",
                "handles as arguments of `typeid` and `static`",
            ),
        ],
    );
}

#[test]
fn static_projects_onto_typeid() {
    rejects(
        &manifest(
            r#"static Unit = "BWAPI::Units" {}
            static Nothing = "BWAPI::Nothing" {}"#,
        ),
        &[
            ("Unit", "a `static` section projects onto a `typeid` type"),
            (
                "Nothing",
                "a `static` section projects onto a `typeid` type",
            ),
        ],
    );
}

// ------------------------------------------------------------------ constants

#[test]
fn const_only_in_static_and_enum() {
    rejects(
        &manifest(
            r#"typeid TechType = "BWAPI::TechType" { const Stim_Packs = 0; }
            handle Game = "BWAPI::Game" { const X = 1; }"#,
        ),
        &[
            (
                "Stim_Packs",
                "`const` is allowed only in `static` and `enum`",
            ),
            ("X", "`const` is allowed only in `static` and `enum`"),
        ],
    );
}

#[test]
fn static_const_names_are_mechanical() {
    rejects(
        &manifest(
            r#"static UnitType = "BWAPI::UnitTypes::Enum" { const Marine = Terran_Marine = 0; }"#,
        ),
        &[("Marine", "named mechanically")],
    );
}

#[test]
fn static_consts_are_accepted() {
    let api = accepts(&manifest(
        r#"static UnitType = "BWAPI::UnitTypes::Enum" {
            const Terran_Marine = 0;
            const None = 228;
            const Also_Zero = 0;
        }"#,
    ));
    let names: Vec<(&str, &str, i32)> = api.statics[0]
        .consts
        .iter()
        .map(|c| (c.rust.0.as_str(), c.cpp.0.as_str(), c.value))
        .collect();
    assert_eq!(
        names,
        [
            ("TERRAN_MARINE", "Terran_Marine", 0),
            ("NONE", "None", 228),
            ("ALSO_ZERO", "Also_Zero", 0),
        ]
    );
}

#[test]
fn const_value_fits_i32() {
    rejects(
        &manifest(r#"static UnitType = "BWAPI::UnitTypes::Enum" { const Big = 2147483648; }"#),
        &[("2147483648", "too large")],
    );
}

#[test]
fn enum_variants() {
    rejects(
        &manifest(
            r#"enum MouseButton = "BWAPI::MouseButton" {
                const M_LEFT = 0;
                const right = M_RIGHT = 1;
                const Middle = M_MIDDLE = 2;
                const Other = M_OTHER = 2;
                fn f() -> i32 = f;
            }"#,
        ),
        &[
            ("M_LEFT", "`const UpperCamelName = CppName = value;`"),
            ("right", "`const UpperCamelName = CppName = value;`"),
            ("2", "value 2 is already taken by `Middle`"),
            ("fn", "an `enum` section holds only `const` declarations"),
        ],
    );
}

#[test]
fn enums_are_accepted() {
    let api = accepts(&manifest(
        r#"enum MouseButton = "BWAPI::MouseButton" {
            const Left = M_LEFT = 0;
            const Right = M_RIGHT = 1;
        }"#,
    ));
    let e = api.enum_("MouseButton");
    let variants: Vec<(&str, &str, i32)> = e
        .variants
        .iter()
        .map(|c| (c.rust.0.as_str(), c.cpp.0.as_str(), c.value))
        .collect();
    assert_eq!(variants, [("Left", "M_LEFT", 0), ("Right", "M_RIGHT", 1)]);
}

// ------------------------------------------------------------------ attributes and skip

#[test]
fn skip_needs_attribute() {
    rejects(
        &game("fn events = getEvents;"),
        &[("events", "a declaration without signature")],
    );
}

#[test]
fn skip_takes_no_other_attributes() {
    rejects(
        &game(
            r#"#[skip = "std::function"]
            #[non_null = "never null"]
            fn events = getEvents;"#,
        ),
        &[("events", "a declaration without signature")],
    );
}

#[test]
fn skip_goes_on_declaration_without_signature() {
    rejects(
        &game(
            r#"#[skip = "duplicate"]
            fn frame() -> i32 = getFrameCount;"#,
        ),
        &[("frame", "`#[skip]` goes on a declaration without signature")],
    );
}

#[test]
fn reasons_are_not_empty() {
    rejects(
        &manifest(
            r#"#[set = ""]
            handle Bullet = "BWAPI::BulletInterface" {
                #[skip = "  "]
                fn client_info = getClientInfo;
            }"#,
        ),
        &[
            (r#""""#, "`#[set]` needs a non-empty reason"),
            (r#""  ""#, "`#[skip]` needs a non-empty reason"),
        ],
    );
}

#[test]
fn unknown_attribute() {
    rejects(
        &game(
            r#"#[stable = "a member"]
            fn frame() -> i32 = getFrameCount;"#,
        ),
        &[("stable", "unknown attribute")],
    );
}

#[test]
fn skips_are_accepted() {
    let api = accepts(&game(
        r#"#[skip = "synchronously rebuilds every set"]
        fn load_snapshot = loadSnapshot;
        fn self_player() -> Option<Player> = self;"#,
    ));
    let game = &api.handle("Game").class;
    assert_eq!(game.skips[0].name.0, "load_snapshot");
    assert_eq!(game.skips[0].cpp.0, "loadSnapshot");
    assert_eq!(game.skips[0].reason, "synchronously rebuilds every set");
    // A C++ name may be a Rust keyword.
    assert_eq!(game.methods[0].cpp.0, "self");
}
