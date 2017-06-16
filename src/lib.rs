extern crate bwapi_sys;

#[cfg(test)] #[macro_use]
extern crate assert_approx_eq;

pub mod position;

pub mod string;
pub mod iterator;

pub mod player;
pub mod game;
pub mod unit;
pub mod region;

pub mod aimodule;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
