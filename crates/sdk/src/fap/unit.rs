// Port of FAP/include/FAP/Unit.hpp of FAP (https://github.com/N00byEdge/FAP, ac3c005).
// Copyright (c) 2018 Hannes Bredberg. MIT License, see LICENSE-FAP.

//! The input and the state of a simulated unit, and the setters of `FAP::Unit`
//! that work out the input from a unit type and its upgrades.

use bwapi::{DamageType, Race, UnitSizeType, UnitType, WeaponType};

/// What FAP does with the unit each frame, decided in C++ by its `unitType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Role {
    /// Attacks the closest enemy it can hit (`unitsim`).
    #[default]
    Fighter,
    /// Heals the closest wounded organic friend (`medicsim`): Terran Medic.
    Medic,
    /// Dies dealing its damage on contact (`suicideSim`): Scourge, Spider Mine,
    /// Infested Terran, Scarab.
    Suicide,
    /// Fights as a fighter and turns into `num_attackers` marines when it dies.
    Bunker {
        /// `Terran_Marine.maxHitPoints()`, in hit points.
        marine_max_health: i32,
    },
    /// Never a combat unit: Protoss Interceptor, whose damage the carrier deals.
    Ignored,
}

/// Regeneration per frame after both players moved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Regen {
    #[default]
    None,
    /// 4/256 hit points: Zerg, except eggs and larvae.
    ZergHp,
    /// 7/256 shields: Protoss.
    ProtossShields,
}

/// Splash radii of the ground weapon in pixels: `innerSplashRadius`,
/// `medianSplashRadius`, `outerSplashRadius`. FAP splashes only for the siege
/// tank in siege mode, and only with `Options { tank_splash: true }`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Splash {
    pub inner: i32,
    pub median: i32,
    pub outer: i32,
}

/// A unit to add: `FAP::Unit` with the values of its setters as fields, in hit
/// points, pixels and frames. Ranges are not squared here.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit<T = ()> {
    pub x: i32,
    pub y: i32,

    pub health: i32,
    pub max_health: i32,
    pub armor: i32,

    pub shields: i32,
    pub max_shields: i32,
    /// `setShieldUpgrades`.
    pub shield_armor: i32,

    /// Pixels per frame.
    pub speed: f32,
    pub flying: bool,
    /// Ground height, or -1 when unknown (`setElevation()`).
    pub elevation: i32,
    pub is_organic: bool,

    pub ground_damage: i32,
    pub ground_cooldown: i32,
    pub ground_max_range: i32,
    pub ground_min_range: i32,
    pub ground_damage_type: DamageType,

    pub air_damage: i32,
    pub air_cooldown: i32,
    pub air_max_range: i32,
    pub air_min_range: i32,
    pub air_damage_type: DamageType,

    pub role: Role,
    pub regen: Regen,
    pub splash: Option<Splash>,
    pub unit_size: UnitSizeType,
    /// Interceptors of a carrier or marines in a bunker.
    pub num_attackers: i32,
    pub attack_cooldown_remaining: i32,

    pub data: T,
}

/// The state of a unit in the simulation: `FAP::FAPUnit`. Hit points and shields
/// are in 1/256, ranges are squared.
#[derive(Clone, Debug, PartialEq)]
pub struct FAPUnit<T = ()> {
    pub x: i32,
    pub y: i32,

    pub health: i32,
    pub max_health: i32,
    pub armor: i32,

    pub shields: i32,
    pub max_shields: i32,
    pub shield_armor: i32,

    pub speed: f32,
    pub speed_squared: f32,
    pub flying: bool,
    pub elevation: i32,

    pub ground_damage: i32,
    pub ground_cooldown: i32,
    pub ground_max_range_squared: i32,
    pub ground_min_range_squared: i32,
    pub ground_damage_type: DamageType,

    pub air_damage: i32,
    pub air_cooldown: i32,
    pub air_max_range_squared: i32,
    pub air_min_range_squared: i32,
    pub air_damage_type: DamageType,

    pub role: Role,
    pub regen: Regen,
    pub splash: Option<Splash>,
    pub unit_size: UnitSizeType,
    pub is_organic: bool,
    pub did_heal_this_frame: bool,

    pub num_attackers: i32,
    pub attack_cooldown_remaining: i32,

    pub data: T,
}

impl<T> From<Unit<T>> for FAPUnit<T> {
    /// What the setters of `FAP::Unit` store: hit points and shields shifted by 8,
    /// the speed and the ranges squared.
    fn from(u: Unit<T>) -> Self {
        FAPUnit {
            x: u.x,
            y: u.y,
            health: u.health << 8,
            max_health: u.max_health << 8,
            armor: u.armor,
            shields: u.shields << 8,
            max_shields: u.max_shields << 8,
            shield_armor: u.shield_armor,
            speed: u.speed,
            speed_squared: u.speed * u.speed,
            flying: u.flying,
            elevation: u.elevation,
            ground_damage: u.ground_damage,
            ground_cooldown: u.ground_cooldown,
            ground_max_range_squared: u.ground_max_range * u.ground_max_range,
            ground_min_range_squared: u.ground_min_range * u.ground_min_range,
            ground_damage_type: u.ground_damage_type,
            air_damage: u.air_damage,
            air_cooldown: u.air_cooldown,
            air_max_range_squared: u.air_max_range * u.air_max_range,
            air_min_range_squared: u.air_min_range * u.air_min_range,
            air_damage_type: u.air_damage_type,
            role: u.role,
            regen: u.regen,
            splash: u.splash,
            unit_size: u.unit_size,
            is_organic: u.is_organic,
            did_heal_this_frame: false,
            num_attackers: u.num_attackers,
            attack_cooldown_remaining: u.attack_cooldown_remaining,
            data: u.data,
        }
    }
}

/// The upgrades and state of a unit beyond its type, as the setters of
/// `FAP::Unit` take them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Upgrades {
    pub armor_upgrades: i32,
    pub attack_upgrades: i32,
    pub shield_upgrades: i32,
    pub speed_upgrade: bool,
    pub attack_speed_upgrade: bool,
    pub range_upgrade: bool,
    pub stimmed: bool,
}

impl<T: Default> Unit<T> {
    /// A whole unit of the type, with the upgrades, at (0, 0) on unknown
    /// ground: the setters of `FAP::Unit` in the order FAP's tests call them
    /// (`testUnit`). `attackers` are the interceptors of a carrier or the
    /// marines of a bunker.
    pub fn of_type(ty: UnitType, attackers: i32, upgrades: &Upgrades) -> Self {
        let (gw, aw) = (ty.ground_weapon(), ty.air_weapon());
        // setUnitType, setHealth, setShields, setFlying, setElevation().
        let mut u = Unit {
            x: 0,
            y: 0,
            max_health: ty.max_hit_points(),
            max_shields: ty.max_shields(),
            shield_armor: 0,
            is_organic: ty.is_organic(),
            speed: ty.top_speed() as f32,
            armor: ty.armor(),
            ground_damage: gw.damage_amount(),
            ground_cooldown: gw.damage_cooldown()
                / (ty.max_ground_hits() * gw.damage_factor()).max(1),
            ground_damage_type: gw.damage_type(),
            ground_min_range: gw.min_range(),
            ground_max_range: gw.max_range(),
            air_damage: aw.damage_amount(),
            air_cooldown: aw.damage_cooldown() / (ty.max_air_hits() * aw.damage_factor()).max(1),
            air_damage_type: aw.damage_type(),
            air_min_range: aw.min_range(),
            air_max_range: aw.max_range(),
            unit_size: ty.size(),
            health: ty.max_hit_points(),
            shields: ty.max_shields(),
            flying: ty.is_flyer(),
            elevation: -1,
            role: role(ty),
            regen: regen(ty),
            splash: (ty == UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE).then(|| Splash {
                inner: gw.inner_splash_radius(),
                median: gw.median_splash_radius(),
                outer: gw.outer_splash_radius(),
            }),
            num_attackers: 0,
            attack_cooldown_remaining: 0,
            data: T::default(),
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

fn regen(ty: UnitType) -> Regen {
    let race = ty.race();
    if race == Race::ZERG {
        let unborn = [
            UnitType::ZERG_EGG,
            UnitType::ZERG_LURKER_EGG,
            UnitType::ZERG_LARVA,
        ];
        if unborn.contains(&ty) {
            Regen::None
        } else {
            Regen::ZergHp
        }
    } else if race == Race::PROTOSS {
        Regen::ProtossShields
    } else {
        Regen::None
    }
}

/// `setAttackerCount`.
fn set_attacker_count<T>(u: &mut Unit<T>, ty: UnitType, num_attackers: i32) {
    u.num_attackers = num_attackers;
    if num_attackers != 0 {
        if ty == UnitType::PROTOSS_CARRIER {
            u.ground_damage = UnitType::PROTOSS_INTERCEPTOR
                .ground_weapon()
                .damage_amount();
            u.air_damage = u.ground_damage;
            // `round` of a positive number.
            u.ground_cooldown = (37.4f32 / num_attackers as f32 + 0.5) as i32;
            u.air_cooldown = u.ground_cooldown;
        }
        if ty == UnitType::TERRAN_BUNKER {
            let gauss = UnitType::TERRAN_MARINE.ground_weapon();
            u.ground_damage = gauss.damage_amount();
            u.air_damage = u.ground_damage;
            u.ground_cooldown = gauss.damage_cooldown() / num_attackers;
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
