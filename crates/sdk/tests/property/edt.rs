//! The exact Euclidean distance transform of a grid: Felzenszwalb and
//! Huttenlocher, "Distance Transforms of Sampled Functions", Theory of
//! Computing 8 (2012). A reference independent of the algorithms it checks.

/// Squared distances stand in for none: large, finite, so that differences of
/// them stay numbers.
const FAR: f64 = 1e20;

/// The lower envelope of the parabolas rooted at `f`: `d[q]` becomes the least
/// `(q - p)² + f[p]`.
fn transform_line(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0;
    v[0] = 0;
    z[0] = -FAR;
    z[1] = FAR;
    for q in 1..n {
        let intersection = |p: usize| {
            ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64)
        };
        let mut s = intersection(v[k]);
        while s <= z[k] {
            k -= 1;
            s = intersection(v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = FAR;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let dq = q as f64 - v[k] as f64;
        *out = dq * dq + f[v[k]];
    }
}

/// The distance from the center of each cell of a grid `width` × `height`, row
/// by row, to the center of the nearest cell where `source` holds; with
/// `ring`, the cells just around the grid are sources too. In cells; `None`
/// where there is no source at all.
pub fn distances(
    width: i32,
    height: i32,
    source: impl Fn(i32, i32) -> bool,
    ring: bool,
) -> Vec<Option<f64>> {
    let pad = i32::from(ring);
    let (w, h) = ((width + 2 * pad) as usize, (height + 2 * pad) as usize);
    let mut grid = vec![FAR; w * h];
    for y in 0..h {
        for x in 0..w {
            let (gx, gy) = (x as i32 - pad, y as i32 - pad);
            let outside = gx < 0 || gy < 0 || gx >= width || gy >= height;
            if (outside && ring) || (!outside && source(gx, gy)) {
                grid[y * w + x] = 0.0;
            }
        }
    }

    let longest = w.max(h);
    let (mut f, mut d) = (vec![0.0; longest], vec![0.0; longest]);
    let (mut v, mut z) = (vec![0; longest], vec![0.0; longest + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        transform_line(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..(y + 1) * w]);
        transform_line(&f[..w], &mut d[..w], &mut v, &mut z);
        grid[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }

    let mut out = Vec::with_capacity((width * height).max(0) as usize);
    for y in 0..height {
        for x in 0..width {
            let squared = grid[(y + pad) as usize * w + (x + pad) as usize];
            out.push((squared < FAR / 2.0).then(|| squared.sqrt()));
        }
    }
    out
}
