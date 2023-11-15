mod decoder;
mod encoder;
mod errors;
mod utils;

use decoder::Decoder;
use encoder::Encoder;
use errors::Error;
use utils::ByteMask;




fn main() {
    let mask = ByteMask::new(2).unwrap();

    let image_path = "default_image.png";
    let secret_path = "pic.png";
    let encoded_path = "encoded.png";
    let decoded_path = "output.png";

    encode(image_path, secret_path,encoded_path, mask).unwrap();
    decode(encoded_path, decoded_path, mask).unwrap();



}


fn encode(image_path: &str, secret_path: &str, output_path: &str, mask: ByteMask)  -> Result<(), Error> {
    let mut encoder = Encoder::new(image_path, secret_path, mask)?;
    encoder.save(output_path)?;
    Ok(())
}

fn decode(encoded_path: &str, decoded_path: &str, mask: ByteMask) -> Result<(), Error> {
    let decoder = Decoder::new(encoded_path, mask)?;
    decoder.save(decoded_path)?;
    Ok(())
}