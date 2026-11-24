//! FAP units from the game: a unit as it is, with the upgrades of its player.

use bwapi::{Context, UnitType, UpgradeType, WeaponType};

use crate::fap::{Unit, Upgrades};

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
    let upgrades = Upgrades {
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
        ..Unit::of_type(ty, attackers, &upgrades)
    }
}
