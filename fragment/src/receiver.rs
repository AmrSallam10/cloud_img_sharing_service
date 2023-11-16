use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::fs;
use std::sync::Arc;
use steganography::encoder::Encoder;

mod fragment;

#[derive(Serialize, Deserialize, Debug)]
struct Image {
    dims: (u32,u32),
    data: Vec<u8>,
}


#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("0.0.0.0:9999").await.expect("Failed to bind to 0.0.0.0:9999");
    let socket = Arc::new(socket);
    let data = fragment::recieve(socket.clone()).await;
    let default_image = image::open("default_image.png").unwrap();
    let encoder = Encoder::new(&data, default_image);
    let encoded_image = encoder.encode_alpha();
    // steganography::util::save_image_buffer(encoded_image, "encoded_image.png".to_string());
    let image = Image{ dims : encoded_image.dimensions(),
        data:encoded_image.into_raw()
    };
    let encoded_bytes = serde_cbor::to_vec(&image).unwrap();
    fragment::send(encoded_bytes, socket.clone(), "0.0.0.0:9099").await;
    
}

