use std::fs::File;
use std::io::{BufWriter, Write};
use image::{ImageBuffer, Rgb};

use crate::errors::Error;
use crate::utils::ByteMask;

pub struct Decoder {
    image: ImageBuffer<Rgb<u8>, Vec<u8>>,
    mask: ByteMask,
}

impl Decoder {
    pub fn new(image_path: &str, mask: ByteMask) -> Result<Self, Error> {
        let image = image::open(image_path)?.to_rgb();
        Ok(Decoder { image, mask })
    }

    pub fn save(&self, output: &str) -> Result<(), Error> {
        let mut secret = BufWriter::new(File::create(output)?);
        let mut chunks = Vec::with_capacity(self.mask.chunks as usize);
        let mut start = false;

        for (i, b) in self.image.iter().map(|b| b & self.mask.mask).enumerate() {
            // Secret starts when we find first non zero byte chunk
            if !start && (b > 0) {
                // The secret should start only at multiples of chunks. Add remaining offset if not the case.
                let n = self.mask.chunks as usize;
                let offset = (self.image.len() - i) % n;
                if offset != 0 {
                    (0..(n - offset)).for_each(|_| chunks.push(0));
                }
                start = true;
            };

            // Save chunk to buffer
            if start {
                chunks.push(b);
            }

            // We can now recover the original byte from the chunks
            if chunks.len() == chunks.capacity() {
                // Recover original byte from LSB chunks
                let byte = self.mask.join_chunks(&chunks);

                // Write recovered byte
                secret.write_all(&[byte])?;

                // Reset the LSB byte chunks buffer
                chunks.clear()
            }
        }

        // Write remaining bytes
        secret.flush()?;
        Ok(())
    }
}