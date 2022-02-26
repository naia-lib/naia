pub use naia_serde_derive::*;

mod consts;
mod error;
mod impls;
mod integer;
mod reader_writer;
mod serde;

pub use error::SerdeErr;
pub use integer::{SignedInteger, SignedVariableInteger, UnsignedInteger, UnsignedVariableInteger};
pub use reader_writer::{BitReader, BitWriter};
pub use serde::Serde;
