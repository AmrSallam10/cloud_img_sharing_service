#![allow(dead_code, unused_variables, unused_imports)]
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
use image::Rgba;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use std::{
    env, fs as std_fs, thread,
    time::{Duration, Instant},
};
use steganography::decoder::Decoder;
use tokio::fs;
use tokio::net::UdpSocket;

mod fragment;

#[derive(Serialize, Deserialize, Debug)]
struct Image {
    dims: (u32, u32),
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
enum Type {
    ClientRequest(u32),
    ElectionRequest(u32),
    OKMsg(u32),
    CoordinatorBrdCast(String),
}

#[derive(Serialize, Deserialize)]
struct Msg {
    sender: SocketAddr,
    receiver: SocketAddr,
    msg_type: Type,
    payload: Option<String>,
}

fn get_servers(filepath: &str) -> Vec<String> {
    let contents =
        std_fs::read_to_string(filepath).expect("Should have been able to read the file");
    let servers: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    servers
}

fn get_req_id_log(filepath: &str) -> u32 {
    match std_fs::read_to_string(filepath) {
        Ok(contents) => contents.parse::<u32>().unwrap_or(1),
        Err(_) => 0, // Default value in case of an error or missing file
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<_> = env::args().collect();
    let ip: &str = args[1].as_str();
    let ip = SocketAddr::from_str(ip).unwrap();
    let servers_filepath = "./servers.txt";
    let req_id_log_filepath = "./req_id_log.txt";
    let servers = get_servers(servers_filepath);
    let mut id = get_req_id_log(req_id_log_filepath);
    let socket = UdpSocket::bind(ip).await.unwrap();
    let mut buffer = [0; 1024];
    let mut chosen_server = String::new();
    // trigger election
    for server in &servers {
        let server = SocketAddr::from_str(server.as_str()).unwrap();
        let msg = Msg {
            sender: socket.local_addr().unwrap(),
            receiver: server,
            msg_type: Type::ClientRequest(id),
            payload: None,
        };
        let serialized_msg = serde_json::to_string(&msg).unwrap();
        socket
            .send_to(serialized_msg.as_bytes(), server)
            .await
            .unwrap();
    }
    match socket.recv_from(&mut buffer).await {
        Ok((bytes_read, src_addr)) => {
            chosen_server = src_addr.to_string();
        }
        Err(e) => {
            eprintln!("Error receiving data: {}", e);
        }
    }
    id += 1;
    std_fs::write(req_id_log_filepath, id.to_string()).unwrap();

    // send the img to be encrypted
    let contents = fs::read("pic.jpg").await.unwrap();
    let socket = Arc::new(socket);
    println!("Message Length {}", contents.len());
    fragment::send(
        contents,
        socket.clone(),
        &chosen_server,
        id.to_string().as_str(),
    )
    .await;
    let data = fragment::recieve(socket.clone()).await;
    let image: Image = serde_cbor::de::from_slice(&data).unwrap();
    let image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>> =
        image::ImageBuffer::from_raw(image.dims.0, image.dims.1, image.data).unwrap();
    steganography::util::save_image_buffer(image_buffer.clone(), "encoded_output.jpeg".to_string());
    let decoder = Decoder::new(image_buffer);
    let secret_bytes = decoder.decode_alpha();
    let _ = tokio::fs::write("secret.jpg", secret_bytes).await;
}
