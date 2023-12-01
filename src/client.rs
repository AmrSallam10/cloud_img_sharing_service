#![allow(dead_code, unused_variables, unused_imports)]
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
use image::{ImageBuffer, Rgba};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::{env, fs as std_fs};
use steganography::decoder::Decoder;
use tokio::fs;
use tokio::net::UdpSocket;

mod fragment;
use fragment::{Image, Msg, Type};
use fragment::{BUFFER_SIZE, ELECTION_PORT, SERVERS_FILEPATH, SERVICE_PORT, SERVICE_SENDBACK_PORT};

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
    let mut buffer = [0; 1024];

    for server in &servers {
        println!("Sending to server {}", server);
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
    for _ in 0..5 {
        match socket.recv_from(&mut buffer).await {
            Ok((_bytes_read, src_addr)) => {
                // println!("{}", src_addr);
                if let Ok(response) = std::str::from_utf8(&buffer[.._bytes_read]) {
                    let chosen_server: SocketAddr = response.parse().unwrap();
                    println!("{}", chosen_server);
                    return Some(chosen_server);
                } else {
                    continue;
                }
            }
            Err(e) => {
                eprintln!("Error getting the result of eletion: {}", e);
            }
        }
    }
    None
}

async fn decode_image(img: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();

    for (_, _, pixel) in img.enumerate_pixels() {
        out.push(pixel[3]);
    }
    out
}

fn create_output_dirs() {
    let base_directory = "output";
    let encoded_directory = "output/encoded";
    let decoded_directory = "output/decoded";

    // Attempt to create the entire directory structure
    if let Err(err) = std_fs::create_dir_all(encoded_directory) {
        if err.kind() == std::io::ErrorKind::AlreadyExists {
            println!("Directory '{}' already exists.", encoded_directory);
        } else {
            println!(
                "Error creating directory '{}': {:?}",
                encoded_directory, err
            );
            return; // Abo  rt if an error occurs
        }
    }

    if let Err(err) = std_fs::create_dir_all(decoded_directory) {
        if err.kind() == std::io::ErrorKind::AlreadyExists {
            println!("Directory '{}' already exists.", decoded_directory);
        } else {
            println!(
                "Error creating directory '{}': {:?}",
                decoded_directory, err
            );
            return; // Abort if an error occurs
        }
    }
}

async fn send_image_request(
    img_id: String,
    requested_access: u32,
    client_socket: Arc<UdpSocket>,
    peer_client_addr: SocketAddr,
) {
    println!("Send Image Request");
    let msg = Msg {
        sender: client_socket.local_addr().unwrap(),
        receiver: peer_client_addr,
        msg_type: Type::ImageRequest(img_id, requested_access),
        payload: None,
    };
    let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
    client_socket
        .send_to(&serialized_msg, peer_client_addr)
        .await
        .unwrap();
}

async fn handel_iamge_request(
    img_id: String,
    img: Image,
    requested_access: u32,
    client_socket: Arc<UdpSocket>,
    src_addr: SocketAddr,
) {
    println!("Handle Image Request");
    let msg = Msg {
        sender: client_socket.local_addr().unwrap(),
        receiver: src_addr,
        msg_type: Type::ShareImage(img_id, img, requested_access),
        payload: None,
    };
    let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
    client_socket
        .send_to(&serialized_msg, src_addr)
        .await
        .unwrap();
}

async fn handel_share_image(
    img_id: String,
    img: Image,
    recieved_access: u32,
    client_socket: Arc<UdpSocket>,
    src_addr: SocketAddr,
) {
    println!("Handle Share Image");
    println!("Image ID: {}", img_id);
    println!("Image Access: {}", recieved_access);
    let image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>> =
        image::ImageBuffer::from_raw(img.dims.0, img.dims.1, img.data).unwrap();
    save_image_buffer(
        image_buffer.clone(),
        format!("output/encoded/{img_id}.jpeg"),
    );
    let secret_bytes = decode_image(image_buffer).await;
    let _ = tokio::fs::write(format!("output/decoded/secret_{img_id}.jpg"), secret_bytes).await;
}

async fn request_update_access(
    img_id: String,
    new_access: u32,
    client_socket: Arc<UdpSocket>,
    peer_client_addr: SocketAddr,
) {
    println!("Send Update Access");
    let msg = Msg {
        sender: client_socket.local_addr().unwrap(),
        receiver: peer_client_addr,
        msg_type: Type::UpdateAccess(img_id, new_access),
        payload: None,
    };
    let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
    client_socket
        .send_to(&serialized_msg, peer_client_addr)
        .await
        .unwrap();
}

async fn handel_update_access(
    img_id: String,
    img: Image,
    new_access: u32,
    client_socket: Arc<UdpSocket>,
    src_addr: SocketAddr,
) {
    println!("Handle Update Access");
    println!("Image ID: {}", img_id);
    println!("Image New Access: {}", new_access);
    let msg = Msg {
        sender: client_socket.local_addr().unwrap(),
        receiver: src_addr,
        msg_type: Type::ShareImage(img_id, img, new_access),
        payload: None,
    };
    let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
    client_socket
        .send_to(&serialized_msg, src_addr)
        .await
        .unwrap();
}

#[tokio::main]
async fn main() {
    let args: Vec<_> = env::args().collect();
    create_output_dirs();
    let mode = args[1].as_str();
    let ip: SocketAddr = args[2]
        .as_str()
        .parse()
        .expect("Failed to parse IP from input");

    //Socket for communication with clients
    let ip_client = SocketAddr::new(ip.ip(), ip.port() + 1);
    let pic_file_path: &str = args[3].as_str();
    let pic_paths = get_pic_paths(pic_file_path);
    let mut id = get_req_id_log(REQ_ID_LOG_FILEPATH);
    let socket = Arc::new(UdpSocket::bind(ip).await.expect("Failed to bind to ip"));
    println!("Cloud Communication on {ip}");
    let client_socket = Arc::new(
        UdpSocket::bind(ip_client)
            .await
            .expect("Failed to bind to ip"),
    );
    println!("Clients Communication on {ip_client}");

    let mut encoded_images: HashMap<String, Image> = HashMap::new();
    let mut service_buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];

    for pic_path in &pic_paths {
        let pic_path = Path::new(pic_path);
        let pic_with_ext = pic_path.file_name().unwrap().to_str().unwrap();
        let pic_without_ext = pic_path.file_stem().unwrap().to_str().unwrap();
        let pic_path = pic_path.to_str().unwrap();
        println!("{:?} - {}", pic_path, pic_without_ext);

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
            encoded_images.insert(msg_id, encoded_image.clone());
            let image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>> = image::ImageBuffer::from_raw(
                encoded_image.dims.0,
                encoded_image.dims.1,
                encoded_image.data,
            )
            .unwrap();
            save_image_buffer(
                image_buffer.clone(),
                format!("output/encoded/encoded_output_{pic_without_ext}.jpeg"),
            );
            let secret_bytes = decode_image(image_buffer).await;
            let _ = tokio::fs::write(
                format!("output/decoded/secret_{pic_with_ext}"),
                secret_bytes,
            )
            .await;

            id += 1;
        } else {
            println!("Failed to get the election result (server ip)");
        };
    }
    std_fs::write(REQ_ID_LOG_FILEPATH, id.to_string())
        .expect("Failed to write req_id to req_id_log.txt");

    let h1 = tokio::spawn({
        async move {
            loop {
                match client_socket.recv_from(&mut service_buffer).await {
                    Ok((bytes_read, src_addr)) => {
                        println!("{} bytes from {}.", bytes_read, src_addr);

                        let msg: Msg = serde_cbor::de::from_slice(&service_buffer[..bytes_read])
                            .expect("Failed to deserialize msg from client socket");

                        match msg.msg_type {
                            Type::ImageRequest(img_id, requested_access) => {
                                handel_iamge_request(
                                    img_id.clone(),
                                    encoded_images.get(&img_id).unwrap().clone(),
                                    requested_access,
                                    client_socket.clone(),
                                    src_addr,
                                )
                                .await;
                            }
                            Type::ShareImage(img_id, img, recieved_access) => {
                                handel_share_image(
                                    img_id,
                                    img,
                                    recieved_access,
                                    client_socket.clone(),
                                    src_addr,
                                )
                                .await;
                            }
                            Type::UpdateAccess(img_id, new_access) => {
                                handel_update_access(
                                    img_id.clone(),
                                    encoded_images.get(&img_id).unwrap().clone(),
                                    new_access,
                                    client_socket.clone(),
                                    src_addr,
                                )
                                .await;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        eprintln!("Error receiving data from client socket: {}", e);
                    }
                }
            }
        }
    });
}
