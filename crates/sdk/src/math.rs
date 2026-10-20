//! Float functions that `core` lacks, with the results of C++ `<cmath>`.

/// The correctly rounded square root, as C++ `std::sqrt`.
#[cfg(all(
    any(target_arch = "x86", target_arch = "x86_64"),
    target_feature = "sse2"
))]
pub(crate) fn sqrt(x: f64) -> f64 {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::{_mm_cvtsd_f64, _mm_set_sd, _mm_sqrt_pd};
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::{_mm_cvtsd_f64, _mm_set_sd, _mm_sqrt_pd};
    // SAFETY: the `cfg` above compiles this only for targets with SSE2 enabled.
    unsafe { _mm_cvtsd_f64(_mm_sqrt_pd(_mm_set_sd(x))) }
}

/// The correctly rounded square root, as C++ `std::sqrt`.
#[cfg(target_arch = "aarch64")]
pub(crate) fn sqrt(x: f64) -> f64 {
    use core::arch::aarch64::{vdup_n_f64, vget_lane_f64, vsqrt_f64};
    // SAFETY: NEON is part of the aarch64 base architecture.
    unsafe { vget_lane_f64::<0>(vsqrt_f64(vdup_n_f64(x))) }
}

/// The correctly rounded square root, as C++ `std::sqrt`, on targets without
/// SSE2 or NEON. wasm32 is one: its `f64.sqrt` is not stable in Rust.
#[cfg(not(any(
    all(
        any(target_arch = "x86", target_arch = "x86_64"),
        target_feature = "sse2"
    ),
    target_arch = "aarch64"
)))]
pub(crate) fn sqrt(x: f64) -> f64 {
    soft_sqrt(x)
}

/// The correctly rounded square root in integers: the root of the significand
/// to 54 bits, rounded to 53 to nearest.
#[cfg(any(
    test,
    not(any(
        all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "sse2"
        ),
        target_arch = "aarch64"
    ))
))]
fn soft_sqrt(x: f64) -> f64 {
    // NaN, ±0 and +∞ are their own roots; below zero there is none.
    if x.is_nan() || x == 0.0 || x == f64::INFINITY {
        return x;
    }
    if x < 0.0 {
        return f64::NAN;
    }
    let bits = x.to_bits();
    let field = (bits >> 52) as i32;
    let fraction = bits & ((1 << 52) - 1);
    // x = m * 2^e with m in [2^52, 2^53), e even after the shift.
    let (mut m, mut e) = if field == 0 {
        let shift = fraction.leading_zeros() - 11;
        (fraction << shift, -1074 - shift as i32)
    } else {
        (fraction | 1 << 52, field - 1075)
    };
    if e % 2 != 0 {
        m <<= 1;
        e -= 1;
    }
    // m * 2^54 is in [2^106, 2^108): its root has 54 bits, one past the result.
    let wide = u128::from(m) << 54;
    let root = wide.isqrt() as u64;
    let inexact = u128::from(root) * u128::from(root) != wide;
    let mut q = root >> 1;
    if root & 1 == 1 && (inexact || q & 1 == 1) {
        q += 1;
    }
    // The root is q * 2^(e/2 - 26), always normal. q is in [2^52, 2^53]: the
    // implicit bit, or a carry to 2^53, adds to the exponent field.
    f64::from_bits((((e / 2 + 1048) as u64) << 52) + q)
}

// `atan`, `atan2` and `fmod` are ports of musl (https://musl.libc.org), MIT
// License, see LICENSE-MUSL. `atan` and `atan2` come to musl from FreeBSD
// msun (s_atan.c, e_atan2.c), under this notice:
//
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//
// Developed at SunSoft, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this
// software is freely granted, provided that this notice
// is preserved.

// The constants by their bits, as musl gives them next to the decimals.
const ATAN_HI: [f64; 4] = [
    f64::from_bits(0x3fdd_ac67_0561_bb4f), // atan(0.5) hi, 4.63647609000806093515e-01
    f64::from_bits(0x3fe9_21fb_5444_2d18), // atan(1.0) hi, 7.85398163397448278999e-01
    f64::from_bits(0x3fef_730b_d281_f69b), // atan(1.5) hi, 9.82793723247329054082e-01
    f64::from_bits(0x3ff9_21fb_5444_2d18), // atan(inf) hi, 1.57079632679489655800e+00
];

const ATAN_LO: [f64; 4] = [
    f64::from_bits(0x3c7a_2b7f_222f_65e2), // atan(0.5) lo, 2.26987774529616870924e-17
    f64::from_bits(0x3c81_a626_3314_5c07), // atan(1.0) lo, 3.06161699786838301793e-17
    f64::from_bits(0x3c70_0788_7af0_cbbd), // atan(1.5) lo, 1.39033110312309984516e-17
    f64::from_bits(0x3c91_a626_3314_5c07), // atan(inf) lo, 6.12323399573676603587e-17
];

const AT: [f64; 11] = [
    f64::from_bits(0x3fd5_5555_5555_550d), // 3.33333333333329318027e-01
    f64::from_bits(0xbfc9_9999_9998_ebc4), // -1.99999999998764832476e-01
    f64::from_bits(0x3fc2_4924_9200_83ff), // 1.42857142725034663711e-01
    f64::from_bits(0xbfbc_71c6_fe23_1671), // -1.11111104054623557880e-01
    f64::from_bits(0x3fb7_45cd_c54c_206e), // 9.09088713343650656196e-02
    f64::from_bits(0xbfb3_b0f2_af74_9a6d), // -7.69187620504482999495e-02
    f64::from_bits(0x3fb1_0d66_a0d0_3d51), // 6.66107313738753120669e-02
    f64::from_bits(0xbfad_de2d_52de_fd9a), // -5.83357013379057348645e-02
    f64::from_bits(0x3fa9_7b4b_2476_0deb), // 4.97687799461593236017e-02
    f64::from_bits(0xbfa2_b444_2c6a_6c2f), // -3.65315727442169155270e-02
    f64::from_bits(0x3f90_ad3a_e322_da11), // 1.62858201153657823623e-02
];

/// π, 3.1415926535897931160e+00, and what it lacks, 1.2246467991473531772e-16.
pub(crate) const PI: f64 = f64::from_bits(0x4009_21fb_5444_2d18);
const PI_LO: f64 = f64::from_bits(0x3ca1_a626_3314_5c07);

/// The high 32 bits of `x`, as `GET_HIGH_WORD` of musl.
fn high_word(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}

/// `atan` of musl: the arc tangent, within 1 ulp.
fn atan(x: f64) -> f64 {
    let ix = high_word(x);
    let sign = ix >> 31 != 0;
    let ix = ix & 0x7fff_ffff;
    let mut x = x;
    let id;
    if ix >= 0x4410_0000 {
        // |x| >= 2^66
        if x.is_nan() {
            return x;
        }
        let z = ATAN_HI[3] + f64::from(f32::from_bits(0x0380_0000)); // 0x1p-120f
        return if sign { -z } else { z };
    }
    if ix < 0x3fdc_0000 {
        // |x| < 0.4375
        if ix < 0x3e40_0000 {
            // |x| < 2^-27
            return x;
        }
        id = None;
    } else {
        x = x.abs();
        if ix < 0x3ff3_0000 {
            // |x| < 1.1875
            if ix < 0x3fe6_0000 {
                // 7/16 <= |x| < 11/16
                id = Some(0);
                x = (2.0 * x - 1.0) / (2.0 + x);
            } else {
                // 11/16 <= |x| < 19/16
                id = Some(1);
                x = (x - 1.0) / (x + 1.0);
            }
        } else if ix < 0x4003_8000 {
            // |x| < 2.4375
            id = Some(2);
            x = (x - 1.5) / (1.0 + 1.5 * x);
        } else {
            // 2.4375 <= |x| < 2^66
            id = Some(3);
            x = -1.0 / x;
        }
    }
    // The sum of AT[i] z^(i+1), split into odd and even polynomials.
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    let Some(id) = id else {
        return x - x * (s1 + s2);
    };
    let z = ATAN_HI[id] - (x * (s1 + s2) - ATAN_LO[id] - x);
    if sign { -z } else { z }
}

/// `atan2` of musl: the angle of the point (`x`, `y`), within 1 ulp. C++
/// `std::atan2` calls the C library of the platform, glibc or the CRT of MSVC,
/// whose last bit may differ: on differences of pixel positions, glibc's in
/// about one case of six.
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    let (ix, lx) = (high_word(x), x.to_bits() as u32);
    let (iy, ly) = (high_word(y), y.to_bits() as u32);
    if (ix.wrapping_sub(0x3ff0_0000) | lx) == 0 {
        // x = 1.0
        return atan(y);
    }
    // 2 * sign(x) + sign(y)
    let m = ((iy >> 31) & 1) | ((ix >> 30) & 2);
    let ix = ix & 0x7fff_ffff;
    let iy = iy & 0x7fff_ffff;

    // y = 0
    if (iy | ly) == 0 {
        return match m {
            0 | 1 => y,
            2 => PI,
            _ => -PI,
        };
    }
    // x = 0
    if (ix | lx) == 0 {
        return if m & 1 != 0 { -PI / 2.0 } else { PI / 2.0 };
    }
    // x is infinite
    if ix == 0x7ff0_0000 {
        return if iy == 0x7ff0_0000 {
            match m {
                0 => PI / 4.0,
                1 => -PI / 4.0,
                2 => 3.0 * PI / 4.0,
                _ => -3.0 * PI / 4.0,
            }
        } else {
            match m {
                0 => 0.0,
                1 => -0.0,
                2 => PI,
                _ => -PI,
            }
        };
    }
    // |y / x| > 2^64
    if ix + (64 << 20) < iy || iy == 0x7ff0_0000 {
        return if m & 1 != 0 { -PI / 2.0 } else { PI / 2.0 };
    }

    // atan(|y / x|) without spurious underflow
    let z = if m & 2 != 0 && iy + (64 << 20) < ix {
        // |y / x| < 2^-64, x < 0
        0.0
    } else {
        atan((y / x).abs())
    };
    match m {
        0 => z,
        1 => -z,
        2 => PI - (z - PI_LO),
        _ => (z - PI_LO) - PI,
    }
}

/// C `round`: to the nearest integer, halfway away from zero. Exact, as
/// `x - trunc(x)` is.
pub(crate) fn round(x: f64) -> f64 {
    // From 2^52 on every double is an integer; NaN and infinities stay.
    if x.is_nan() || x.abs() >= 4_503_599_627_370_496.0 {
        return x;
    }
    let truncated = (x as i64) as f64;
    let rest = x - truncated;
    if rest >= 0.5 {
        truncated + 1.0
    } else if rest <= -0.5 {
        truncated - 1.0
    } else if truncated == 0.0 {
        // -0.4 rounds to -0.0, as in C.
        0.0 * x
    } else {
        truncated
    }
}

/// `fmod` of musl: the remainder of `x / y` with the sign of `x`, exact.
pub(crate) fn fmod(x: f64, y: f64) -> f64 {
    let mut uxi = x.to_bits();
    let mut uyi = y.to_bits();
    let mut ex = (uxi >> 52 & 0x7ff) as i32;
    let mut ey = (uyi >> 52 & 0x7ff) as i32;
    let sx = uxi >> 63;

    if uyi << 1 == 0 || y.is_nan() || ex == 0x7ff {
        // musl divides x * y by itself for NaN and the invalid exception,
        // which Rust does not see.
        return f64::NAN;
    }
    if uxi << 1 <= uyi << 1 {
        if uxi << 1 == uyi << 1 {
            return 0.0 * x;
        }
        return x;
    }

    // Normalize x and y.
    if ex == 0 {
        let mut i = uxi << 12;
        while i >> 63 == 0 {
            ex -= 1;
            i <<= 1;
        }
        uxi <<= -ex + 1;
    } else {
        uxi &= u64::MAX >> 12;
        uxi |= 1 << 52;
    }
    if ey == 0 {
        let mut i = uyi << 12;
        while i >> 63 == 0 {
            ey -= 1;
            i <<= 1;
        }
        uyi <<= -ey + 1;
    } else {
        uyi &= u64::MAX >> 12;
        uyi |= 1 << 52;
    }

    // x mod y
    while ex > ey {
        let i = uxi.wrapping_sub(uyi);
        if i >> 63 == 0 {
            if i == 0 {
                return 0.0 * x;
            }
            uxi = i;
        }
        uxi <<= 1;
        ex -= 1;
    }
    let i = uxi.wrapping_sub(uyi);
    if i >> 63 == 0 {
        if i == 0 {
            return 0.0 * x;
        }
        uxi = i;
    }
    while uxi >> 52 == 0 {
        uxi <<= 1;
        ex -= 1;
    }

    // Scale the result.
    if ex > 0 {
        uxi -= 1 << 52;
        uxi |= (ex as u64) << 52;
    } else {
        uxi >>= -ex + 1;
    }
    uxi |= sx << 63;
    f64::from_bits(uxi)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    /// Equal bits, or both NaN.
    fn same(a: f64, b: f64) -> bool {
        a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
    }

    /// Both NaN, or of one sign and at most 1 ulp apart.
    fn within_ulp(a: f64, b: f64) -> bool {
        (a.is_nan() && b.is_nan())
            || (a.is_sign_negative() == b.is_sign_negative()
                && a.to_bits().abs_diff(b.to_bits()) <= 1)
    }

    /// What BWEB makes of an angle of `getAngle`, which adds 2π to a negative
    /// one: the multiple of π/4 it rounds to, of 0.785 (`defenseArrangement`),
    /// and the side of each bound of `findSecondaryLocations`.
    fn bweb_keys(angle: f64) -> (f64, f64, [bool; 4]) {
        let angle = if angle < 0.0 {
            angle + 2.0 * super::PI
        } else {
            angle
        };
        let bounds = [0.785, 2.355, 3.925, 5.495].map(|b| angle < b);
        (
            (angle / (super::PI / 4.0)).round(),
            (angle / 0.785).round(),
            bounds,
        )
    }

    /// Edge values of both signs: zeros, the smallest subnormal and normal,
    /// small integers, values around 1, the largest finite, infinities, NaN.
    fn edges() -> Vec<f64> {
        let positive = [
            0.0,
            f64::from_bits(1),
            f64::MIN_POSITIVE,
            1e-300,
            0.5,
            1.0 - f64::EPSILON / 2.0,
            1.0,
            1.0 + f64::EPSILON,
            1.5,
            2.0,
            3.0,
            7.0,
            32.0,
            12345.678,
            1e300,
            f64::MAX,
            f64::INFINITY,
        ];
        let mut all: Vec<f64> = positive.iter().flat_map(|&x| [x, -x]).collect();
        all.push(f64::NAN);
        all
    }

    #[test]
    fn sqrt_matches_std() {
        for x in [0.0, 1.0, 2.0, 3.0, 1e-300, 12345.678, 1e300] {
            assert_eq!(super::sqrt(x), std::primitive::f64::sqrt(x));
        }
    }

    #[test]
    fn soft_sqrt_matches_std() {
        let check = |x: f64| {
            let (ours, theirs) = (super::soft_sqrt(x), x.sqrt());
            assert!(same(ours, theirs), "sqrt({x:e}): {ours:e}, std {theirs:e}");
        };
        edges().into_iter().for_each(check);
        // Subnormals, and squares with their neighbours: the root is exact,
        // or just off a representable value.
        (1..=64).map(f64::from_bits).for_each(check);
        for i in 0..=4096u64 {
            let square = (i * i) as f64;
            for x in [
                square,
                f64::from_bits(square.to_bits() + 1),
                f64::from_bits(square.to_bits().wrapping_sub(1)),
            ] {
                check(x);
            }
        }
        // Positive doubles of every exponent, by a fixed xorshift.
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..1_000_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            check(f64::from_bits(state >> 1));
        }
    }

    #[test]
    fn fmod_matches_std_at_the_edges() {
        for x in edges() {
            for y in edges() {
                let (ours, theirs) = (super::fmod(x, y), x % y);
                assert!(
                    same(ours, theirs),
                    "fmod({x:e}, {y:e}): {ours:e}, std {theirs:e}"
                );
            }
        }
    }

    #[test]
    fn fmod_matches_std_on_angles() {
        // What the station presets of BWEB take: angles and their sums, mod π.
        for i in -800..=800 {
            let x = f64::from(i) * 0.0123456789;
            for y in [super::PI, super::PI / 2.0, super::PI / 4.0] {
                let (ours, theirs) = (super::fmod(x, y), x % y);
                assert!(
                    same(ours, theirs),
                    "fmod({x:e}, {y:e}): {ours:e}, std {theirs:e}"
                );
            }
        }
    }

    #[test]
    fn round_matches_std() {
        let halves = (-40..=40).map(|i| f64::from(i) / 4.0);
        let near = [
            0.49999999999999994,
            -0.49999999999999994,
            2.5000000000000004,
            4503599627370495.5,
        ];
        for x in edges().into_iter().chain(halves).chain(near) {
            let (ours, theirs) = (super::round(x), x.round());
            assert!(same(ours, theirs), "round({x:e}): {ours:e}, std {theirs:e}");
        }
    }

    #[test]
    fn atan2_is_within_an_ulp_of_std_at_the_edges() {
        for y in edges() {
            for x in edges() {
                let (ours, theirs) = (super::atan2(y, x), y.atan2(x));
                assert!(
                    within_ulp(ours, theirs),
                    "atan2({y:e}, {x:e}): {ours:e}, std {theirs:e}"
                );
            }
        }
    }

    #[test]
    fn atan2_gives_bweb_the_angles_of_std_on_pixel_differences() {
        // What `getAngle` of BWEB takes: differences of positions in pixels,
        // the y one negated. Every difference up to 300 pixels, then every
        // 29th up to the size of the largest map. In the last bit musl and
        // glibc differ on about one in six; what BWEB rounds them to does not.
        let near = -300..=300;
        let far = (-8192..=8192).step_by(29);
        let mut differ = 0;
        for range in [near.collect::<Vec<i32>>(), far.collect()] {
            for &dy in &range {
                for &dx in &range {
                    let (y, x) = (-f64::from(dy), f64::from(dx));
                    let (ours, theirs) = (super::atan2(y, x), y.atan2(x));
                    assert!(
                        within_ulp(ours, theirs),
                        "atan2({y}, {x}): {ours:e}, std {theirs:e}"
                    );
                    assert_eq!(bweb_keys(ours), bweb_keys(theirs), "atan2({y}, {x})");
                    differ += usize::from(!same(ours, theirs));
                }
            }
        }
        assert!(
            differ > 0,
            "musl and glibc agree everywhere: the comments here are out of date"
        );
    }
}
