//! A grid of cells over one of the three position scales, stored row by row.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::ops::{Index, IndexMut};

use bwapi::{Position, TilePosition, WalkPosition};

/// A position a [`Grid`] is indexed by.
pub trait GridPosition: Copy {
    fn new(x: i32, y: i32) -> Self;
    fn x(self) -> i32;
    fn y(self) -> i32;
}

macro_rules! grid_position {
    ($($t:ty),*) => {$(
        impl GridPosition for $t {
            fn new(x: i32, y: i32) -> Self {
                <$t>::new(x, y)
            }
            fn x(self) -> i32 {
                self.x
            }
            fn y(self) -> i32 {
                self.y
            }
        }
    )*};
}

grid_position!(Position, TilePosition, WalkPosition);

/// `width × height` cells, indexed by positions of the scale `P`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid<P, T> {
    width: i32,
    height: i32,
    cells: Box<[T]>,
    _scale: PhantomData<fn(P) -> P>,
}

impl<P: GridPosition, T> Grid<P, T> {
    /// A grid filled row by row with `f(position)`. Negative sizes count as 0.
    pub fn from_fn(width: i32, height: i32, mut f: impl FnMut(P) -> T) -> Self {
        let (width, height) = (width.max(0), height.max(0));
        let mut cells = Vec::with_capacity(width as usize * height as usize);
        for y in 0..height {
            for x in 0..width {
                cells.push(f(P::new(x, y)));
            }
        }
        Grid {
            width,
            height,
            cells: cells.into_boxed_slice(),
            _scale: PhantomData,
        }
    }

    /// A grid with every cell equal to `value`.
    pub fn filled(width: i32, height: i32, value: T) -> Self
    where
        T: Clone,
    {
        Self::from_fn(width, height, |_| value.clone())
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn contains(&self, p: P) -> bool {
        (0..self.width).contains(&p.x()) && (0..self.height).contains(&p.y())
    }

    pub fn get(&self, p: P) -> Option<&T> {
        self.contains(p).then(|| &self.cells[self.index(p)])
    }

    pub fn get_mut(&mut self, p: P) -> Option<&mut T> {
        if self.contains(p) {
            let i = self.index(p);
            Some(&mut self.cells[i])
        } else {
            None
        }
    }

    /// The cells row by row.
    pub fn cells(&self) -> &[T] {
        &self.cells
    }

    fn index(&self, p: P) -> usize {
        p.y() as usize * self.width as usize + p.x() as usize
    }
}

/// Panics outside the grid; for positions already checked.
impl<P: GridPosition, T> Index<P> for Grid<P, T> {
    type Output = T;

    fn index(&self, p: P) -> &T {
        debug_assert!(self.contains(p));
        &self.cells[Grid::index(self, p)]
    }
}

impl<P: GridPosition, T> IndexMut<P> for Grid<P, T> {
    fn index_mut(&mut self, p: P) -> &mut T {
        debug_assert!(self.contains(p));
        let i = Grid::index(self, p);
        &mut self.cells[i]
    }
}
