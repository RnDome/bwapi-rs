//! FAP in C++ (`FAP_DIR`): what the Rust port of FAP is compared with. It needs
//! no game.

use std::ffi::c_void;

/// A unit of a scenario: `testUnit` of FAP's tests at a position, with its
/// health, shields and upgrades.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    /// 1 or 2.
    pub player: i32,
    /// `UnitType::id()`.
    pub type_id: i32,
    pub x: i32,
    pub y: i32,
    pub health: i32,
    pub shields: i32,
    pub armor_upgrades: i32,
    pub attack_upgrades: i32,
    pub shield_upgrades: i32,
    /// 1 speed, 2 attack speed, 4 range upgrade, 8 stimmed.
    pub flags: i32,
    /// Ground height, -1 for unknown.
    pub elevation: i32,
}

/// What the comparison reads of a `FAPUnit`, in this order: x, y, health,
/// max health, armor, shields, max shields, shield armor, the bits of speed
/// and of its square, flying, elevation; ground damage, cooldown, squared max
/// and min range and damage type id; the same for air; unit size id, organic,
/// healed this frame, attackers, attack cooldown remaining.
pub type Unit = [i32; 27];

/// Both players' units after a step.
pub type State = [Vec<Unit>; 2];

type Sink = extern "C" fn(*mut c_void, *const i32, usize);

unsafe extern "C" {
    fn openbw_host_fap_reference(
        specs: *const Spec,
        count: usize,
        tank_splash: i32,
        steps: i32,
        frames: i32,
        ctx: *mut c_void,
        sink: Sink,
    );
}

unsafe extern "C" {
    fn openbw_host_fap_run(
        specs: *const Spec,
        count: usize,
        tank_splash: i32,
        mode: i32,
        frames: i32,
        ctx: *mut c_void,
        sink: Sink,
    );
}

/// A run of C++ FAP for its properties, each unit tagged with its index in
/// `specs`: the messages of `openbw_host_fap_run` (`cpp/fap_reference.cpp`),
/// the outcome of a call to `simulate`, then the units of both players with
/// their tags. `mode` and `frames` choose the run, as there.
pub fn run(specs: &[Spec], tank_splash: bool, mode: i32, frames: i32) -> Vec<Vec<i32>> {
    let mut raw: Vec<Vec<i32>> = Vec::new();
    // SAFETY: `specs` outlives the call; `collect` reads `raw` only during it.
    unsafe {
        openbw_host_fap_run(
            specs.as_ptr(),
            specs.len(),
            i32::from(tank_splash),
            mode,
            frames,
            (&raw mut raw).cast(),
            collect,
        )
    };
    raw
}

extern "C" fn collect(ctx: *mut c_void, values: *const i32, len: usize) {
    // SAFETY: `ctx` is the `Vec` of `simulate`; C++ passes `len` readable values.
    let states = unsafe { &mut *ctx.cast::<Vec<Vec<i32>>>() };
    states.push(unsafe { std::slice::from_raw_parts(values, len) }.to_vec());
}

/// Adds the combat units of `specs` to C++ FAP, then simulates `steps` times
/// `frames` frames (a negative number: until the fight is over) with or
/// without tank splash. The state before the first step and after each.
pub fn simulate(specs: &[Spec], tank_splash: bool, steps: i32, frames: i32) -> Vec<State> {
    let mut raw: Vec<Vec<i32>> = Vec::new();
    // SAFETY: `specs` outlives the call; `collect` reads `raw` only during it.
    unsafe {
        openbw_host_fap_reference(
            specs.as_ptr(),
            specs.len(),
            i32::from(tank_splash),
            steps,
            frames,
            (&raw mut raw).cast(),
            collect,
        )
    };
    raw.iter()
        .map(|v| {
            let mut rest = v.as_slice();
            [0, 1].map(|_| {
                let (&n, tail) = rest.split_first().expect("a count of units");
                let (units, tail) = tail.split_at(n as usize * 27);
                rest = tail;
                units
                    .chunks_exact(27)
                    .map(|c| c.try_into().expect("27 fields"))
                    .collect()
            })
        })
        .collect()
}
