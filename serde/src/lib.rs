pub use naia_serde_derive::*;

mod reader_writer;
mod consts;
mod error;
mod impls;
mod traits;
mod integer;

pub use error::DeErr;
pub use traits::{Ser, De};
pub use reader_writer::{BitReader, BitWriter};
pub use integer::{UnsignedInteger, UnsignedVariableInteger, SignedVariableInteger, SignedInteger};
