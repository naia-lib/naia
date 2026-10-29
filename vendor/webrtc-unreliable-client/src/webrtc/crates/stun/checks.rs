use crate::webrtc::stun::attributes::*;
use crate::webrtc::stun::error::*;

use subtle::ConstantTimeEq;

// check_size returns ErrAttrSizeInvalid if got is not equal to expected.
pub(crate) fn check_size(_at: AttrType, got: usize, expected: usize) -> Result<()> {
    if got == expected {
        Ok(())
    } else {
        Err(Error::ErrAttributeSizeInvalid)
    }
}

pub(crate) fn check_hmac(got: &[u8], expected: &[u8]) -> Result<()> {
    if got.ct_eq(expected).unwrap_u8() != 1 {
        Err(Error::ErrIntegrityMismatch)
    } else {
        Ok(())
    }
}

// check_overflow returns ErrAttributeSizeOverflow if got is bigger that max.
pub(crate) fn check_overflow(_at: AttrType, got: usize, max: usize) -> Result<()> {
    if got <= max {
        Ok(())
    } else {
        Err(Error::ErrAttributeSizeOverflow)
    }
}
