use image::{open, DynamicImage, ImageBuffer, Rgba};

use crate::fragment::Image;

pub async fn encode_img(
    secret_bytes: Vec<u8>,
    req_id: String,
    default_image: DynamicImage,
) -> Vec<u8> {
    let (send, receive) = tokio::sync::oneshot::channel();
    rayon::spawn(move || {
        println!(
            "[{}] Started encryption. Image size {}",
            req_id,
            secret_bytes.len()
        );
        // let encoder = Encoder::new(&data, default_image);
        // let encoded_image = encoder.encode_alpha();

        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = default_image.into_rgba8();
        let (width, height) = img.dimensions();
        let bytes = width * height;

        if secret_bytes.len() > bytes as usize {
            panic!("secret_bytes is too large for image size");
        }

        let mut encoded_image = img.clone();

        for (x, y, pixel) in encoded_image.enumerate_pixels_mut() {
            let secret_bytes_index = x + (y * width);

            if secret_bytes_index < secret_bytes.len() as u32 {
                pixel[3] = secret_bytes[secret_bytes_index as usize];
            } else {
                // If secret bytes are exhausted, break out of the loop
                break;
            }
        }

        let image = Image {
            dims: encoded_image.dimensions(),
            data: encoded_image.into_raw(),
        };
        let encoded_bytes = serde_cbor::to_vec(&image).unwrap();
        let _ = send.send(encoded_bytes);
    });

    receive.await.expect("Rayon Panicked [encrption]")
}

pub async fn decode_img(img: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();

    for (_, _, pixel) in img.enumerate_pixels() {
        out.push(pixel[3]);
    }
    out
}
