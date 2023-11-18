#![allow(dead_code, unused_variables, unused_imports)]
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
use image::{ImageBuffer, Rgba};
use std::net::SocketAddr;
use std::sync::Arc;
use std::{env, fs as std_fs};
use steganography::decoder::Decoder;
use tokio::fs;
use tokio::net::UdpSocket;

mod fragment;
use fragment::{Image, Msg, Type};
use fragment::{ELECTION_PORT, SERVERS_FILEPATH, SERVICE_PORT, SERVICE_SENDBACK_PORT};

const REQ_ID_LOG_FILEPATH: &str = "./req_id_log.txt";

fn get_servers(filepath: &str, mode: &str) -> Vec<String> {
    let contents =
        std_fs::read_to_string(filepath).expect("Should have been able to read the file");
    if mode == "local" {
        let servers: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
        servers
    } else {
        let servers: Vec<String> = contents
            .lines()
            .map(|s| format!("{}:{}", s, ELECTION_PORT))
            .collect();
        servers
    }
}

fn get_req_id_log(filepath: &str) -> u32 {
    match std_fs::read_to_string(filepath) {
        Ok(contents) => contents.parse::<u32>().unwrap_or(1),
        Err(_) => 0, // Default value in case of an error or missing file
    }
}

fn save_image_buffer(image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>>, filename: String) {
    let image = image::DynamicImage::ImageRgba8(image_buffer);
    image.save(filename).unwrap();
}

fn get_pic_paths(filepath: &str) -> Vec<String> {
    let contents =
        std_fs::read_to_string(filepath).expect("Should have been able to read the file");
    let pic_paths: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    pic_paths
}

async fn send_init_request_to_cloud(
    socket: Arc<UdpSocket>,
    id: u32,
    mode: &str,
) -> Option<SocketAddr> {
    let servers = get_servers(SERVERS_FILEPATH, mode);
    println!("{:?}", servers);
    let mut buffer = [0; 1024];

    for server in &servers {
        let server_ip: SocketAddr = server
            .parse()
            .expect("Failed to parse server ip from servers.tx");
        let msg = Msg {
            sender: socket.local_addr().unwrap(),
            receiver: server_ip,
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

    // wait for election result (server ip)
    match socket.recv_from(&mut buffer).await {
        Ok((_bytes_read, src_addr)) => {
            // println!("{}", src_addr);
            let response: &str =
                std::str::from_utf8(&buffer[.._bytes_read]).expect("Failed to convert to UTF-8");
            let chosen_server: SocketAddr = response.parse().unwrap();
            println!("{}", chosen_server);
            Some(chosen_server)
        }
        Err(e) => {
            eprintln!("Error getting the result of eletion: {}", e);
            None
        }
    }
}

async fn decode_image(img: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();

    for (_, _, pixel) in img.enumerate_pixels() {
        out.push(pixel[3]);
    }
    out
}

#[tokio::main]
async fn main() {
    let args: Vec<_> = env::args().collect();
    let mode = args[1].as_str();
    let ip: SocketAddr = args[2]
        .as_str()
        .parse()
        .expect("Failed to parse IP from input");
    let pic_file_path: &str = args[3].as_str();
    let pic_paths = get_pic_paths(pic_file_path);
    let mut id = get_req_id_log(REQ_ID_LOG_FILEPATH);
    let socket = Arc::new(UdpSocket::bind(ip).await.expect("Failed to bind to ip"));
    for pic_path in &pic_paths {
        println!("Started img {}", pic_path);
        let pic_path_without_ext = pic_path.split('.').next().expect("Failed to split on '.'");

        // trigger election
        if let Some(chosen_server) = send_init_request_to_cloud(socket.clone(), id, mode).await {
            let chosen_server = chosen_server.to_string();
            println!("Sending img to sever {}", chosen_server);

            // send the img to be encrypted
            let contents = fs::read(pic_path).await.unwrap();
            let msg_id = format!("{}:{}", socket.local_addr().unwrap(), id);
            // println!("Message Length {}", contents.len());
            fragment::client_send(contents, socket.clone(), &chosen_server, &msg_id).await;
            println!("Finished sending pic");
            let encoded_bytes = fragment::recieve(socket.clone()).await;
            let encoded_image: Image = serde_cbor::de::from_slice(&encoded_bytes).unwrap();
            let image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>> = image::ImageBuffer::from_raw(
                encoded_image.dims.0,
                encoded_image.dims.1,
                encoded_image.data,
            )
            .unwrap();
            save_image_buffer(
                image_buffer.clone(),
                format!("encoded_output_{pic_path_without_ext}.jpeg"),
            );
            // let decoder = Decoder::new(image_buffer);
            // let secret_bytes = decoder.decode_alpha();
            let secret_bytes = decode_image(image_buffer).await;
            let _ =
                tokio::fs::write(format!("secret_{pic_path_without_ext}.jpg"), secret_bytes).await;

            id += 1;
        } else {
            println!("Failed to get the election result (server ip)");
        };
    }
    std_fs::write(REQ_ID_LOG_FILEPATH, id.to_string())
        .expect("Failed to write req_id to req_id_log.txt");
}
