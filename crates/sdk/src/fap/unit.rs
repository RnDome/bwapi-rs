// Port of FAP/include/FAP/Unit.hpp of FAP (https://github.com/N00byEdge/FAP, ac3c005).
// Copyright (c) 2018 Hannes Bredberg. MIT License, see LICENSE-FAP.

//! The input and the state of a simulated unit.

/// `BWAPI::DamageType`: how damage scales with the size of the target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DamageType {
    Independent,
    Explosive,
    Concussive,
    #[default]
    Normal,
    IgnoreArmor,
    None,
    Unknown,
}

/// `BWAPI::UnitSizeType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UnitSizeType {
    Independent,
    #[default]
    Small,
    Medium,
    Large,
    None,
    Unknown,
}

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
#[derive(Clone, Debug, Default, PartialEq)]
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
#[derive(Clone, Debug, Default, PartialEq)]
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
