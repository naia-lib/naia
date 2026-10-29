use std::io::{Read, Write};

use super::content::*;
use crate::webrtc::dtls::error::Result;

// Application data messages are carried by the record layer and are
// fragmented, compressed, and encrypted based on the current connection
// state.  The messages are treated as transparent data to the record
// layer.
// https://tools.ietf.org/html/rfc5246#section-10
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct ApplicationData {
    pub(crate) data: Vec<u8>,
}

impl ApplicationData {
    pub(crate) fn content_type(&self) -> ContentType {
        ContentType::ApplicationData
    }

    pub(crate) fn size(&self) -> usize {
        self.data.len()
    }

    pub(crate) fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_all(&self.data)?;

        Ok(writer.flush()?)
    }

    pub(crate) fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let mut data: Vec<u8> = vec![];
        reader.read_to_end(&mut data)?;

        Ok(ApplicationData { data })
    }
}
