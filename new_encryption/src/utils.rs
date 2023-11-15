use crate::errors::Error;

/// Represents a number of bits in a byte, its range is limited to [0, 8]
/// Implements Iterator to iterate through non-overlapping masked bits of the byte
#[derive(Copy, Clone)]
pub struct ByteMask {
    pub bits: u8,
    pub mask: u8,
    pub chunks: u8,
    padded: bool,
    byte: u8,
    step: u8,
}

impl ByteMask {
    pub fn new(bits: u8) -> Result<Self, Error> {
        if (bits == 0) || (bits > 8) {
            Err(Error::InvalidNumberOfBits)
        } else {
            let mask = (u16::pow(2, bits as u32) - 1) as u8;
            let chunks = f32::ceil(8.0 / bits as f32) as u8;
            let padded = 8 < (chunks * bits);

            Ok(ByteMask {
                bits,
                mask,
                chunks,
                byte: 0,
                step: 0,
                padded,
            })
        }
    }

    /// Sets the byte for which to iter over
    pub fn set_byte(&mut self, byte: u8) -> Self {
        self.byte = byte;
        self.step = 0;
        *self
    }

    /// Inverse process, given a set of masked bytes, returns the original byte
    pub fn join_chunks<'a, T>(self, chunks: &'a T) -> u8
    where
        &'a T: IntoIterator<Item = &'a u8>,
    {
        let mut byte = 0;
        let mut shift: u8 = 8;

        for chunk in chunks {
            shift = shift.saturating_sub(self.bits);
            byte |= chunk << shift;
        }

        byte
    }
}

impl Iterator for ByteMask {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.step >= self.chunks {
            return None;
        }

        self.step += 1;

        if self.padded && (self.step == self.chunks) {
            let shift = self.bits * self.step - 8;
            Some(self.byte & (self.mask >> shift))
        } else {
            let shift = 8 - self.bits * self.step;
            Some((self.byte >> shift) & self.mask)
        }
    }
}