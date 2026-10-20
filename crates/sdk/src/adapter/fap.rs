// The setters of FAP/include/FAP/Unit.hpp of FAP
// (https://github.com/N00byEdge/FAP, ac3c005), which read BWAPI type data.
// Copyright (c) 2018 Hannes Bredberg. MIT License, see LICENSE-FAP.

//! FAP units from BWAPI: what the setters of `FAP::Unit` work out of a unit
//! type and its upgrades.

use bwapi::{Context, Race, UnitType, UpgradeType, WeaponType};

use crate::fap::{DamageType, Regeneration, Role, Splash, Unit, UnitSizeType};

/// The upgrades and state of a unit beyond its type, as the setters of
/// `FAP::Unit` take them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FapUpgrades {
    pub armor_upgrades: i32,
    pub attack_upgrades: i32,
    pub shield_upgrades: i32,
    pub speed_upgrade: bool,
    pub attack_speed_upgrade: bool,
    pub range_upgrade: bool,
    pub stimmed: bool,
}

/// A whole unit of the type, with the upgrades, at (0, 0) on unknown ground:
/// the setters of `FAP::Unit` in the order FAP's tests call them (`testUnit`).
/// `attackers` are the interceptors of a carrier or the marines of a bunker.
pub fn fap_unit_of_type<T: Default>(
    ty: UnitType,
    attackers: i32,
    upgrades: &FapUpgrades,
) -> Unit<T> {
    let (gw, aw) = (ty.ground_weapon(), ty.air_weapon());
    // setUnitType, setHealth, setShields, setFlying, setElevation().
    let mut u = Unit {
        max_health: ty.max_hit_points(),
        max_shields: ty.max_shields(),
        is_organic: ty.is_organic(),
        speed: ty.top_speed() as f32,
        armor: ty.armor(),
        ground_damage: gw.damage_amount(),
        ground_cooldown: gw.damage_cooldown() / (ty.max_ground_hits() * gw.damage_factor()).max(1),
        ground_damage_type: damage_type(gw.damage_type()),
        ground_min_range: gw.min_range(),
        ground_max_range: gw.max_range(),
        air_damage: aw.damage_amount(),
        air_cooldown: aw.damage_cooldown() / (ty.max_air_hits() * aw.damage_factor()).max(1),
        air_damage_type: damage_type(aw.damage_type()),
        air_min_range: aw.min_range(),
        air_max_range: aw.max_range(),
        unit_size: unit_size(ty.size()),
        health: ty.max_hit_points(),
        shields: ty.max_shields(),
        flying: ty.is_flyer(),
        elevation: -1,
        role: role(ty),
        regeneration: regeneration(ty),
        splash: (ty == UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE).then(|| Splash {
            inner: gw.inner_splash_radius(),
            median: gw.median_splash_radius(),
            outer: gw.outer_splash_radius(),
        }),
        ..Unit::default()
    };

    set_attacker_count(&mut u, ty, attackers);
    u.attack_cooldown_remaining = 0;
    u.armor += upgrades.armor_upgrades;
    set_attack_upgrades(&mut u, ty, upgrades.attack_upgrades);
    u.shield_armor = upgrades.shield_upgrades;
    if upgrades.speed_upgrade {
        u.speed *= speed_upgrade_factor(ty);
    }
    if upgrades.attack_speed_upgrade {
        u.ground_cooldown -= u.ground_cooldown / 4;
        u.air_cooldown -= u.air_cooldown / 4;
    }
    set_range_upgrade(&mut u, ty, upgrades.range_upgrade);
    if upgrades.stimmed {
        u.ground_cooldown >>= 1;
        u.air_cooldown >>= 1;
        u.speed *= 1.5;
    }
    u
}

/// The unit as it is in the game: its type with the upgrades of its player and
/// its stim, where it stands, on which ground, with its hit points, shields,
/// cooldown, and interceptors or marines.
pub fn fap_unit<T: Default>(cx: Context<'_, '_>, unit: bwapi::Unit<'_>, data: T) -> Unit<T> {
    let (ty, player) = (unit.unit_type(), unit.player());
    let weapon = if ty.ground_weapon() != WeaponType::NONE {
        ty.ground_weapon()
    } else {
        ty.air_weapon()
    };
    let upgrades = FapUpgrades {
        armor_upgrades: player.upgrade_level(ty.armor_upgrade()),
        attack_upgrades: player.upgrade_level(weapon.upgrade_type()),
        shield_upgrades: player.upgrade_level(UpgradeType::PROTOSS_PLASMA_SHIELDS),
        speed_upgrade: player.top_speed(ty) > ty.top_speed(),
        attack_speed_upgrade: player.weapon_damage_cooldown(ty) < weapon.damage_cooldown(),
        // Charon Boosters reach the air weapon of a goliath only.
        range_upgrade: [ty.ground_weapon(), ty.air_weapon()]
            .into_iter()
            .any(|w| player.weapon_max_range(w) > w.max_range()),
        stimmed: unit.is_stimmed(),
    };
    let attackers = if ty == UnitType::PROTOSS_CARRIER {
        unit.interceptor_count()
    } else if ty == UnitType::TERRAN_BUNKER {
        unit.loaded_units().len() as i32
    } else {
        0
    };
    let at = unit.position();
    Unit {
        x: at.x,
        y: at.y,
        health: unit.hit_points(),
        shields: unit.shields(),
        flying: unit.is_flying(),
        elevation: cx.game().ground_height(unit.tile_position()),
        attack_cooldown_remaining: unit
            .ground_weapon_cooldown()
            .max(unit.air_weapon_cooldown()),
        data,
        ..fap_unit_of_type(ty, attackers, &upgrades)
    }
}

/// What FAP does by the type of the unit.
fn role(ty: UnitType) -> Role {
    if ty == UnitType::PROTOSS_INTERCEPTOR {
        Role::Ignored
    } else if ty == UnitType::TERRAN_MEDIC {
        Role::Medic
    } else if [
        UnitType::ZERG_SCOURGE,
        UnitType::TERRAN_VULTURE_SPIDER_MINE,
        UnitType::ZERG_INFESTED_TERRAN,
        UnitType::PROTOSS_SCARAB,
    ]
    .contains(&ty)
    {
        Role::Suicide
    } else if ty == UnitType::TERRAN_BUNKER {
        Role::Bunker {
            marine_max_health: UnitType::TERRAN_MARINE.max_hit_points(),
        }
    } else {
        Role::Fighter
    }
}

fn regeneration(ty: UnitType) -> Regeneration {
    let race = ty.race();
    if race == Race::ZERG {
        let unborn = [
            UnitType::ZERG_EGG,
            UnitType::ZERG_LURKER_EGG,
            UnitType::ZERG_LARVA,
        ];
        if unborn.contains(&ty) {
            Regeneration::None
        } else {
            Regeneration::ZergHealth
        }
    } else if race == Race::PROTOSS {
        Regeneration::ProtossShields
    } else {
        Regeneration::None
    }
}

fn damage_type(t: bwapi::DamageType) -> DamageType {
    [
        (bwapi::DamageType::INDEPENDENT, DamageType::Independent),
        (bwapi::DamageType::EXPLOSIVE, DamageType::Explosive),
        (bwapi::DamageType::CONCUSSIVE, DamageType::Concussive),
        (bwapi::DamageType::NORMAL, DamageType::Normal),
        (bwapi::DamageType::IGNORE_ARMOR, DamageType::IgnoreArmor),
        (bwapi::DamageType::NONE, DamageType::None),
    ]
    .into_iter()
    .find_map(|(b, f)| (b == t).then_some(f))
    .unwrap_or(DamageType::Unknown)
}

fn unit_size(t: bwapi::UnitSizeType) -> UnitSizeType {
    [
        (bwapi::UnitSizeType::INDEPENDENT, UnitSizeType::Independent),
        (bwapi::UnitSizeType::SMALL, UnitSizeType::Small),
        (bwapi::UnitSizeType::MEDIUM, UnitSizeType::Medium),
        (bwapi::UnitSizeType::LARGE, UnitSizeType::Large),
        (bwapi::UnitSizeType::NONE, UnitSizeType::None),
    ]
    .into_iter()
    .find_map(|(b, f)| (b == t).then_some(f))
    .unwrap_or(UnitSizeType::Unknown)
}

/// `setAttackerCount`.
fn set_attacker_count<T>(u: &mut Unit<T>, ty: UnitType, attacker_count: i32) {
    u.attacker_count = attacker_count;
    if attacker_count != 0 {
        if ty == UnitType::PROTOSS_CARRIER {
            u.ground_damage = UnitType::PROTOSS_INTERCEPTOR
                .ground_weapon()
                .damage_amount();
            u.air_damage = u.ground_damage;
            // `round` of a positive number.
            u.ground_cooldown = (37.4f32 / attacker_count as f32 + 0.5) as i32;
            u.air_cooldown = u.ground_cooldown;
        }
        if ty == UnitType::TERRAN_BUNKER {
            let gauss = UnitType::TERRAN_MARINE.ground_weapon();
            u.ground_damage = gauss.damage_amount();
            u.air_damage = u.ground_damage;
            u.ground_cooldown = gauss.damage_cooldown() / attacker_count;
            u.air_cooldown = u.ground_cooldown;
        }
    } else if ty == UnitType::PROTOSS_CARRIER || ty == UnitType::TERRAN_BUNKER {
        u.ground_damage = 0;
        u.air_damage = 0;
    }
    if ty == UnitType::PROTOSS_REAVER {
        u.ground_damage = UnitType::PROTOSS_SCARAB.ground_weapon().damage_amount();
    }
}

/// `setAttackUpgrades`.
fn set_attack_upgrades<T>(u: &mut Unit<T>, ty: UnitType, level: i32) {
    if u.ground_damage == 0 && u.air_damage == 0 {
        return;
    }
    let upgraded = |w: WeaponType| w.damage_amount() + w.damage_bonus() * level;
    if ty == UnitType::PROTOSS_CARRIER {
        u.ground_damage = upgraded(UnitType::PROTOSS_INTERCEPTOR.ground_weapon());
        u.air_damage = upgraded(UnitType::PROTOSS_INTERCEPTOR.air_weapon());
    } else if ty == UnitType::TERRAN_BUNKER {
        u.ground_damage = upgraded(UnitType::TERRAN_MARINE.ground_weapon());
        u.air_damage = upgraded(UnitType::TERRAN_MARINE.air_weapon());
    } else if ty == UnitType::PROTOSS_REAVER {
        u.ground_damage = upgraded(UnitType::PROTOSS_SCARAB.ground_weapon());
    } else {
        u.ground_damage += ty.ground_weapon().damage_bonus() * level;
        u.air_damage += ty.air_weapon().damage_bonus() * level;
    }
}

/// The factor of `setSpeedUpgrade`.
fn speed_upgrade_factor(ty: UnitType) -> f32 {
    let x1_5 = [
        UnitType::ZERG_ZERGLING,
        UnitType::ZERG_HYDRALISK,
        UnitType::ZERG_ULTRALISK,
        UnitType::PROTOSS_SHUTTLE,
        UnitType::PROTOSS_OBSERVER,
        UnitType::PROTOSS_ZEALOT,
        UnitType::TERRAN_VULTURE,
    ];
    if x1_5.contains(&ty) {
        1.5
    } else if ty == UnitType::PROTOSS_SCOUT {
        4.0 / 3.0
    } else if ty == UnitType::ZERG_OVERLORD {
        4.0
    } else {
        1.0
    }
}

/// `setRangeUpgrade`, without squaring: `FastAPproximation` squares the ranges.
fn set_range_upgrade<T>(u: &mut Unit<T>, ty: UnitType, range_upgraded: bool) {
    let extra_range = |w: WeaponType| {
        if w == WeaponType::HELLFIRE_MISSILE_PACK {
            32 * 3
        } else if w == WeaponType::PHASE_DISRUPTOR {
            32 * 2
        } else if w == WeaponType::GAUSS_RIFLE || w == WeaponType::NEEDLE_SPINES {
            32
        } else {
            0
        }
    };
    let index = usize::from(range_upgraded)
        + usize::from(ty == UnitType::TERRAN_BUNKER) * 2
        + usize::from(ty == UnitType::PROTOSS_CARRIER) * 4;
    let ground = [
        u.ground_max_range,
        u.ground_max_range + extra_range(ty.ground_weapon()),
        5 * 32,
        6 * 32,
        32 * 8,
        32 * 8,
    ];
    u.ground_max_range = ground[index];
    let air = [
        u.air_max_range,
        u.air_max_range + extra_range(ty.air_weapon()),
        5 * 32,
        6 * 32,
        32 * 8,
        32 * 8,
    ];
    u.air_max_range = air[index];
}
