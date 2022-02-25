pub use naia_serde_derive::*;

mod reader_writer;
mod consts;
mod error;
mod impls;
mod serde;
mod integer;

pub use error::SerdeErr;
pub use serde::Serde;
pub use reader_writer::{BitReader, BitWriter};
pub use integer::{UnsignedInteger, UnsignedVariableInteger, SignedVariableInteger, SignedInteger};
