//! chitu2lys: converts CHITUBOX projects (`.chitubox`) into Lychee Slicer scenes (`.lys`). Used by the
//! `chitu2lys` command-line tool and the `chitu2lys-gui` window.

pub mod convert;
mod ground;
mod lys;
mod trees;

pub use convert::{Report, convert};
