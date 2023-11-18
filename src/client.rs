#![allow(dead_code, unused_variables, unused_imports)]
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
use image::Rgba;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::{env, fs as std_fs};
use steganography::decoder::Decoder;
use tokio::fs;
use tokio::net::UdpSocket;

mod fragment;
use fragment::{Image, Msg, Type};

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
    let pic_path: &str = args[2].as_str();
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
        // let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
        let serialized_msg = serde_json::to_string(&msg).unwrap();
        socket
            .send_to(serialized_msg.as_bytes(), server)
            .await
            .unwrap();
    }

    match socket.recv_from(&mut buffer).await {
        Ok((bytes_read, src_addr)) => {
            chosen_server = src_addr.to_string();
            // let response: &str =
            //     std::str::from_utf8(&buffer[..bytes_read]).expect("Failed to convert to UTF-8");
            // let msg: String = serde_json::from_str(response).unwrap();
            println!("{}", chosen_server);
        }
        Err(e) => {
            eprintln!("Error receiving data: {}", e);
        }
    }
    // id += 1;
    std_fs::write(req_id_log_filepath, id.to_string()).unwrap();

    // send the img to be encrypted
    let contents = fs::read(pic_path).await.unwrap();
    let socket = Arc::new(socket);
    let msg_id = format!("{}:{}", socket.local_addr().unwrap(), id);
    println!("Message Length {}", contents.len());
    fragment::client_send(contents, socket.clone(), &chosen_server, &msg_id).await;
    println!("Finished sending pic");
    let data = fragment::recieve(socket.clone()).await;
    let image: Image = serde_cbor::de::from_slice(&data).unwrap();
    let image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>> =
        image::ImageBuffer::from_raw(image.dims.0, image.dims.1, image.data).unwrap();
    steganography::util::save_image_buffer(
        image_buffer.clone(),
        format!("encoded_output_{pic_path}.jpeg"),
    );
    let decoder = Decoder::new(image_buffer);
    let secret_bytes = decoder.decode_alpha();
    let _ = tokio::fs::write(format!("secret_{pic_path}.jpg"), secret_bytes).await;
}
