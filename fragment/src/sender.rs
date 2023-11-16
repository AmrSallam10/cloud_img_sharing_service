use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::fs;
use std::sync::Arc;
use image::Rgba;
use steganography::decoder::Decoder;

mod fragment;

#[derive(Serialize, Deserialize, Debug)]
struct Image {
    dims: (u32,u32),
    data: Vec<u8>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>>{
    let socket = UdpSocket::bind("0.0.0.0:9099").await.expect("Couldn't bind to 0.0.0.0:9099");
    
    let contents = fs::read("pic.jpg").await?;
    let socket = Arc::new(socket);
    println!("Message Lenght {}", contents.len());
    fragment::send(contents, socket.clone(), "0.0.0.0:9999").await;
    let data = fragment::recieve(socket.clone()).await;
    let image: Image = serde_cbor::de::from_slice(&data).unwrap();
    let image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>> = image::ImageBuffer::from_raw(image.dims.0, image.dims.1, image.data).unwrap();
    steganography::util::save_image_buffer(image_buffer.clone(), "encoded_output_1.jpeg".to_string());
    let decoder = Decoder::new(image_buffer);
    let secret_bytes = decoder.decode_alpha();
    let _ = tokio::fs::write("secret_2.jpg", secret_bytes).await;

    Ok(())
}
