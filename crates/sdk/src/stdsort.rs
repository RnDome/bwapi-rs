//! `std::sort` and the heap functions as libstdc++ implements them (GCC 11,
//! `bits/stl_algo.h` and `bits/stl_heap.h`): introsort with the median of three,
//! insertion sort of runs up to 16 elements and heap sort past the depth limit.
//!
//! They are not stable, and the results of the originals depend on the order
//! they leave equal elements in: BWEM's altitudes (through the bookkeeping of
//! the sea side mini tiles) and areas (through the order mini tiles are added),
//! the paths of BWEB's A* and JPS (through the order of equal costs in their
//! open lists). The port orders exactly as the originals built with GCC do, so
//! it can be checked against them; the order is the same on every platform the
//! port runs on.

/// Sorts `v` by `less`, leaving equal elements where libstdc++ leaves them.
pub(crate) fn sort<T: Copy>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
    if v.is_empty() {
        return;
    }
    let depth_limit = 2 * lg(v.len());
    introsort_loop(v, 0, v.len(), depth_limit, &mut less);
    final_insertion_sort(v, &mut less);
}

/// Insertion sort below this many elements.
const THRESHOLD: usize = 16;

/// `std::__lg`: the floor of the base 2 logarithm of `n > 0`.
fn lg(n: usize) -> usize {
    (usize::BITS - 1 - n.leading_zeros()) as usize
}

fn introsort_loop<T: Copy>(
    v: &mut [T],
    first: usize,
    mut last: usize,
    mut depth_limit: usize,
    less: &mut impl FnMut(&T, &T) -> bool,
) {
    while last - first > THRESHOLD {
        if depth_limit == 0 {
            heap_sort(&mut v[first..last], less);
            return;
        }
        depth_limit -= 1;
        let cut = unguarded_partition_pivot(v, first, last, less);
        introsort_loop(v, cut, last, depth_limit, less);
        last = cut;
    }
}

fn unguarded_partition_pivot<T: Copy>(
    v: &mut [T],
    first: usize,
    last: usize,
    less: &mut impl FnMut(&T, &T) -> bool,
) -> usize {
    let mid = first + (last - first) / 2;
    move_median_to_first(v, first, first + 1, mid, last - 1, less);
    unguarded_partition(v, first + 1, last, first, less)
}

fn move_median_to_first<T: Copy>(
    v: &mut [T],
    result: usize,
    a: usize,
    b: usize,
    c: usize,
    less: &mut impl FnMut(&T, &T) -> bool,
) {
    let median = if less(&v[a], &v[b]) {
        if less(&v[b], &v[c]) {
            b
        } else if less(&v[a], &v[c]) {
            c
        } else {
            a
        }
    } else if less(&v[a], &v[c]) {
        a
    } else if less(&v[b], &v[c]) {
        c
    } else {
        b
    };
    v.swap(result, median);
}

fn unguarded_partition<T: Copy>(
    v: &mut [T],
    mut first: usize,
    mut last: usize,
    pivot: usize,
    less: &mut impl FnMut(&T, &T) -> bool,
) -> usize {
    loop {
        while less(&v[first], &v[pivot]) {
            first += 1;
        }
        last -= 1;
        while less(&v[pivot], &v[last]) {
            last -= 1;
        }
        if first >= last {
            return first;
        }
        v.swap(first, last);
        first += 1;
    }
}

fn final_insertion_sort<T: Copy>(v: &mut [T], less: &mut impl FnMut(&T, &T) -> bool) {
    if v.len() > THRESHOLD {
        insertion_sort(v, 0, THRESHOLD, less);
        for i in THRESHOLD..v.len() {
            unguarded_linear_insert(v, i, less);
        }
    } else {
        insertion_sort(v, 0, v.len(), less);
    }
}

fn insertion_sort<T: Copy>(
    v: &mut [T],
    first: usize,
    last: usize,
    less: &mut impl FnMut(&T, &T) -> bool,
) {
    if first == last {
        return;
    }
    for i in first + 1..last {
        if less(&v[i], &v[first]) {
            let value = v[i];
            v.copy_within(first..i, first + 1);
            v[first] = value;
        } else {
            unguarded_linear_insert(v, i, less);
        }
    }
}

/// Moves `v[last]` down past the greater elements before it; one that is not
/// greater is known to be there.
fn unguarded_linear_insert<T: Copy>(
    v: &mut [T],
    mut last: usize,
    less: &mut impl FnMut(&T, &T) -> bool,
) {
    let value = v[last];
    let mut next = last - 1;
    while less(&value, &v[next]) {
        v[last] = v[next];
        last = next;
        if next == 0 {
            break;
        }
        next -= 1;
    }
    v[last] = value;
}

/// `std::__partial_sort(first, last, last)`: make a heap, then sort it.
fn heap_sort<T: Copy>(v: &mut [T], less: &mut impl FnMut(&T, &T) -> bool) {
    make_heap(v, &mut *less);
    let mut last = v.len();
    while last > 1 {
        last -= 1;
        // `__pop_heap(first, last, last)`.
        let value = v[last];
        v[last] = v[0];
        adjust_heap(v, 0, last, value, less);
    }
}

/// `std::make_heap`: `less` puts the greatest element on top.
pub(crate) fn make_heap<T: Copy>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
    let len = v.len();
    if len < 2 {
        return;
    }
    let mut parent = (len - 2) / 2;
    loop {
        let value = v[parent];
        adjust_heap(v, parent, len, value, &mut less);
        if parent == 0 {
            break;
        }
        parent -= 1;
    }
}

/// `std::push_heap`: adds the last element of `v` to the heap before it.
pub(crate) fn push_heap<T: Copy>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
    let Some(last) = v.len().checked_sub(1) else {
        return;
    };
    let value = v[last];
    let mut hole = last;
    while hole > 0 {
        let parent = (hole - 1) / 2;
        if !less(&v[parent], &value) {
            break;
        }
        v[hole] = v[parent];
        hole = parent;
    }
    v[hole] = value;
}

/// `std::pop_heap`: moves the top of the heap to the end of `v` and makes a
/// heap of the rest.
pub(crate) fn pop_heap<T: Copy>(v: &mut [T], mut less: impl FnMut(&T, &T) -> bool) {
    let len = v.len();
    if len > 1 {
        let last = len - 1;
        let value = v[last];
        v[last] = v[0];
        adjust_heap(v, 0, last, value, &mut less);
    }
}

fn adjust_heap<T: Copy>(
    v: &mut [T],
    mut hole: usize,
    len: usize,
    value: T,
    less: &mut impl FnMut(&T, &T) -> bool,
) {
    let top = hole;
    let mut second_child = hole;
    while second_child < (len - 1) / 2 {
        second_child = 2 * (second_child + 1);
        if less(&v[second_child], &v[second_child - 1]) {
            second_child -= 1;
        }
        v[hole] = v[second_child];
        hole = second_child;
    }
    if len & 1 == 0 && second_child == (len - 2) / 2 {
        second_child = 2 * (second_child + 1);
        v[hole] = v[second_child - 1];
        hole = second_child - 1;
    }
    // `__push_heap`.
    while hole > top {
        let parent = (hole - 1) / 2;
        if !less(&v[parent], &value) {
            break;
        }
        v[hole] = v[parent];
        hole = parent;
    }
    v[hole] = value;
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    #[test]
    fn sorts_like_any_sort() {
        // Lengths across the insertion sort threshold and deep enough for the
        // heap sort fallback on adversarial runs.
        let mut seed = 12345u32;
        for len in [0, 1, 2, 3, 15, 16, 17, 33, 100, 1000, 5000] {
            let mut v: Vec<u32> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    seed % 50
                })
                .collect();
            let mut expected = v.clone();
            expected.sort();
            super::sort(&mut v, |a, b| a < b);
            assert_eq!(v, expected, "length {len}");
        }
        let mut organ_pipe: Vec<i32> = (0..2000).map(|i: i32| 1000 - (i - 1000).abs()).collect();
        let mut expected = organ_pipe.clone();
        expected.sort();
        super::sort(&mut organ_pipe, |a, b| a < b);
        assert_eq!(organ_pipe, expected);
    }

    #[test]
    fn heaps_pop_the_greatest_first() {
        let less = |a: &u32, b: &u32| a < b;
        let mut seed = 777u32;
        let values: Vec<u32> = (0..500)
            .map(|_| {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                seed % 100
            })
            .collect();
        let mut expected = values.clone();
        expected.sort_by(|a, b| b.cmp(a));

        let drain = |mut heap: Vec<u32>| {
            let mut popped = Vec::new();
            while !heap.is_empty() {
                super::pop_heap(&mut heap, less);
                popped.push(heap.pop().unwrap());
            }
            popped
        };
        let mut pushed = Vec::new();
        for &x in &values {
            pushed.push(x);
            super::push_heap(&mut pushed, less);
        }
        assert_eq!(drain(pushed), expected);
        let mut made = values.clone();
        super::make_heap(&mut made, less);
        assert_eq!(drain(made), expected);
    }
}
