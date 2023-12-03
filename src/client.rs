#![allow(
    dead_code,
    unused_variables,
    unused_imports,
    clippy::redundant_allocation,
    unused_assignments
)]

extern crate serde;
extern crate serde_derive;
extern crate serde_json;
use image::{open, ImageBuffer, Rgba};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::{env, fs as std_fs};
use steganography::decoder::Decoder;
use tokio::fs;
use tokio::net::UdpSocket;

use crate::commons::{
    self, ENCRYPTED_PICS_PATH, HIGH_RES_PICS_PATH, LOW_RES_PICS_PATH, PICS_ROOT_PATH,
    REQ_ID_LOG_FILEPATH,
};
use crate::dir_of_service::ClientDirOfService;
use crate::encryption::{decode_img, encode_img};
use crate::fragment::{self, BigMessage};
use crate::utils::{
    create_output_dirs, file_exists, get_cloud_servers, get_pic_paths, get_req_id_log, mkdir,
};
use commons::{Msg, Type};
use commons::{BUFFER_SIZE, ELECTION_PORT, SERVERS_FILEPATH, SERVICE_PORT, SERVICE_SENDBACK_PORT};
use fragment::Image;

pub struct ClientBackend {
    cloud_socket: Arc<UdpSocket>,
    client_socket: Arc<UdpSocket>,
    next_req_id: u32,
    mode: String,
    cloud_servers: Vec<(SocketAddr, SocketAddr)>,
    dir_of_serv: ClientDirOfService,
    received_complete_imgs: HashMap<String, BigMessage>,
}

impl ClientBackend {
    pub async fn new(args: Vec<String>) -> ClientBackend {
        let mode = args[1].as_str();
        let ip_to_cloud: SocketAddr = args[2]
            .as_str()
            .parse()
            .expect("Failed to parse IP from input");

        let ip_to_clients = SocketAddr::new(ip_to_cloud.ip(), ip_to_cloud.port() + 1);
        let next_req_id = get_req_id_log(REQ_ID_LOG_FILEPATH);

        let cloud_socket: Arc<UdpSocket> = Arc::new(
            UdpSocket::bind(ip_to_cloud)
                .await
                .expect("Failed to bind to ip"),
        );
        println!("Cloud Communication on {ip_to_cloud}");

        let client_socket = Arc::new(
            UdpSocket::bind(ip_to_clients)
                .await
                .expect("Failed to bind to ip"),
        );
        println!("Clients Communication on {ip_to_clients}");

        let cloud_servers = get_cloud_servers(SERVERS_FILEPATH, mode);

        let dir_of_serv = ClientDirOfService::new();
        let received_complete_imgs = HashMap::new();

        ClientBackend {
            cloud_socket,
            client_socket,
            next_req_id,
            mode: String::from(mode),
            cloud_servers,
            dir_of_serv,
            received_complete_imgs,
        }
    }

    pub async fn init(&self) {
        let mut clients_buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let client_socket = self.client_socket.clone();
        ClientDirOfService::join(self.cloud_socket.clone(), self.cloud_servers.clone()).await;
        let mut received_complete_imgs: HashMap<String, BigMessage> = HashMap::new();
        let mut encoded_images: HashMap<String, Image> = HashMap::new();

        let h1 = tokio::spawn({
            async move {
                loop {
                    match client_socket.recv_from(&mut clients_buffer).await {
                        Ok((bytes_read, src_addr)) => {
                            println!("{} bytes from {}.", bytes_read, src_addr);

                            let msg: Msg =
                                serde_cbor::de::from_slice(&clients_buffer[..bytes_read])
                                    .expect("Failed to deserialize msg from client socket");

                            ClientBackend::handle_msg_from_client(
                                client_socket.clone(),
                                msg,
                                &mut encoded_images,
                                src_addr,
                                &mut received_complete_imgs,
                            )
                            .await;
                        }
                        Err(e) => {
                            eprintln!("Error receiving data from client socket: {}", e);
                        }
                    }
                }
            }
        });
    }

    async fn handle_msg_from_client(
        client_socket: Arc<UdpSocket>,
        msg: Msg,
        encoded_images: &mut HashMap<String, Image>,
        src_addr: SocketAddr,
        received_complete_imgs: &mut HashMap<String, BigMessage>,
    ) {
        match msg.msg_type {
            Type::LowResImgReq => {
                ClientBackend::handle_low_res_imgs_req(client_socket.clone(), src_addr).await;
            }

            Type::LowResImgReply(frag) => {
                ClientBackend::handle_low_res_imgs_reply(
                    client_socket,
                    frag,
                    src_addr,
                    received_complete_imgs,
                )
                .await;
            }

            Type::Fragment(frag) => {
                if let Some(req_id) = fragment::receive_one(
                    client_socket.clone(),
                    frag,
                    src_addr,
                    received_complete_imgs,
                )
                .await
                {
                    let data = received_complete_imgs.get(&req_id).unwrap().data.to_owned();
                    received_complete_imgs.remove(&req_id);

                    if let Ok(Msg {
                        msg_type: Type::SharedImage(img_id, img, recieved_access),
                        ..
                    }) = serde_cbor::de::from_slice(&data)
                    {
                        ClientBackend::handle_shared_image(
                            req_id,
                            img,
                            recieved_access,
                            client_socket.clone(),
                            src_addr,
                        )
                        .await;
                    }
                }
            }
            Type::ImageRequest(img_id, requested_access) => {
                ClientBackend::handle_image_request(
                    img_id,
                    requested_access,
                    client_socket.clone(),
                    src_addr,
                )
                .await;
            }

            // Type::SharedImage(img_id, img, recieved_access) => {
            //     ClientBackend::handle_shared_image(
            //         img_id,
            //         img,
            //         recieved_access,
            //         client_socket.clone(),
            //         src_addr,
            //     )
            //     .await;
            // }
            Type::UpdateAccess(img_id, new_access) => {
                ClientBackend::handle_update_access(
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

    pub async fn request_low_res_images(&self, client_addr: SocketAddr) {
        println!("Send Low Res Image Request");
        let msg = Msg {
            sender: self.client_socket.local_addr().unwrap(),
            receiver: client_addr,
            msg_type: Type::LowResImgReq,
            payload: None,
        };
        let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
        self.client_socket
            .send_to(&serialized_msg, client_addr)
            .await
            .unwrap();
    }

    async fn handle_low_res_imgs_req(client_socket: Arc<UdpSocket>, src_addr: SocketAddr) {
        let pics_file_path: &str = "./pics1.txt";
        let pics = get_pic_paths(pics_file_path);

        for pic in &pics {
            let path = format!("{}/{}", LOW_RES_PICS_PATH, pic);
            println!("{}", path);

            let pic_bin = fs::read(path).await.unwrap();
            // src unique
            let pic_id = format!("{}&{}", client_socket.local_addr().unwrap(), pic);
            fragment::client_send(
                pic_bin,
                client_socket.clone(),
                src_addr.to_string().as_str(),
                pic_id.as_str(),
                true,
            )
            .await;
            println!("Finished sending pic");
        }
    }

    async fn handle_low_res_imgs_reply(
        client_socket: Arc<UdpSocket>,
        frag: fragment::Fragment,
        src_addr: SocketAddr,
        received_complete_imgs: &mut HashMap<String, BigMessage>,
    ) {
        if let Some(pic_id) = fragment::receive_one(
            client_socket.clone(),
            frag,
            src_addr,
            received_complete_imgs,
        )
        .await
        {
            let data = received_complete_imgs.get(&pic_id).unwrap().data.to_owned();
            received_complete_imgs.remove(&pic_id);
            let path = format!("{}/{}", LOW_RES_PICS_PATH, src_addr);
            let parts: Vec<&str> = pic_id.split('&').collect();
            let pic_name = *parts.last().unwrap();
            mkdir(path.as_str());
            fs::write(format!("{}/{}", path, pic_name), data)
                .await
                .unwrap();
        }
    }

    async fn send_init_request_to_cloud(&self, t: Type) -> Option<SocketAddr> {
        let socket = self.cloud_socket.clone();
        let mode = self.mode.clone();

        let servers = self.cloud_servers.clone();
        let mut buffer = [0; 1024];

        for server in &servers {
            let target_addr = server.1;

            println!("Sending to server {:?}", server);
            let msg = Msg {
                sender: socket.local_addr().unwrap(),
                receiver: target_addr,
                msg_type: t.clone(),
                payload: None,
            };
            let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
            // let serialized_msg = serde_json::to_string(&msg).unwrap();
            socket.send_to(&serialized_msg, target_addr).await.unwrap();
        }

        // wait for election result (server ip)
        for _ in 0..5 {
            match socket.recv_from(&mut buffer).await {
                Ok((_bytes_read, src_addr)) => {
                    // println!("{}", src_addr);
                    if let Ok(response) = std::str::from_utf8(&buffer[.._bytes_read]) {
                        let chosen_server: SocketAddr = response.parse().unwrap();
                        // println!("{}", chosen_server);
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

    pub async fn send_image_request(
        &self,
        img_id: u32,
        requested_access: u32,
        peer_client_addr: SocketAddr,
    ) {
        println!("Send Image Request");
        let msg = Msg {
            sender: self.client_socket.local_addr().unwrap(),
            receiver: peer_client_addr,
            msg_type: Type::ImageRequest(img_id, requested_access),
            payload: None,
        };
        let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
        self.client_socket
            .send_to(&serialized_msg, peer_client_addr)
            .await
            .unwrap();
    }

    async fn handle_image_request(
        img_id: u32,
        requested_access: u32,
        client_socket: Arc<UdpSocket>,
        src_addr: SocketAddr,
    ) {
        println!("Handle Image Request");
        let path = format!("{}/pic{}.jpg", ENCRYPTED_PICS_PATH, img_id);

        let img_buffer = file_as_image_buffer(path);
        let image = Image {
            dims: img_buffer.dimensions(),
            data: img_buffer.into_raw(),
        };

        let msg = Msg {
            sender: client_socket.local_addr().unwrap(),
            receiver: src_addr,
            msg_type: Type::SharedImage(img_id, image, requested_access),
            payload: None,
        };

        let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
        let pic_id = format!(
            "{}&{}&pic{}.jpg",
            client_socket.local_addr().unwrap(),
            src_addr,
            img_id
        );

        fragment::client_send(
            serialized_msg,
            client_socket.clone(),
            src_addr.to_string().as_str(),
            pic_id.as_str(),
            false,
        )
        .await;
        println!("Finished sending pic");
    }

    async fn handle_shared_image(
        pic_id: String,
        img: Image,
        recieved_access: u32,
        client_socket: Arc<UdpSocket>,
        src_addr: SocketAddr,
    ) {
        println!("Handle Share Image");
        println!("Image ID: {}", pic_id);
        println!("Image Access: {}", recieved_access);
        let path_encrypted = format!("{}/{}", ENCRYPTED_PICS_PATH, src_addr);
        let path_decoded = format!("{}/{}", HIGH_RES_PICS_PATH, src_addr);
        let parts: Vec<&str> = pic_id.split('&').collect();
        let pic_name = *parts.last().unwrap();
        mkdir(path_encrypted.as_str());
        mkdir(path_decoded.as_str());

        let image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>> =
            image::ImageBuffer::from_raw(img.dims.0, img.dims.1, img.data).unwrap();

        save_image_buffer(
            image_buffer.clone(),
            format!("{}/{}", path_encrypted, pic_name),
        );

        let secret_bytes = decode_img(image_buffer).await;
        let _ = tokio::fs::write(format!("{}/{}", path_decoded, pic_name), secret_bytes).await;
    }

    pub async fn request_update_access(
        &self,
        img_id: String,
        new_access: u32,
        peer_client_addr: SocketAddr,
    ) {
        println!("Send Update Access");
        let msg = Msg {
            sender: self.client_socket.local_addr().unwrap(),
            receiver: peer_client_addr,
            msg_type: Type::UpdateAccess(img_id, new_access),
            payload: None,
        };
        let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
        self.client_socket
            .send_to(&serialized_msg, peer_client_addr)
            .await
            .unwrap();
    }

    async fn handle_update_access(
        img_id: String,
        img: Image,
        new_access: u32,
        client_socket: Arc<UdpSocket>,
        src_addr: SocketAddr,
    ) {
        println!("Handle Update Access");
        println!("Image ID: {}", img_id);
        println!("Image New Access: {}", new_access);
        // let msg = Msg {
        //     sender: client_socket.local_addr().unwrap(),
        //     receiver: src_addr,
        //     msg_type: Type::SharedImage(img_id, img, new_access),
        //     payload: None,
        // };
        // let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
        // client_socket
        //     .send_to(&serialized_msg, src_addr)
        //     .await
        //     .unwrap();
    }

    pub async fn query_dir_of_serv(&self) -> Option<HashMap<SocketAddr, bool>> {
        if let Some(chosen_server) = self
            .send_init_request_to_cloud(Type::ClientRequest(1))
            .await
        {
            ClientDirOfService::query(self.cloud_socket.clone(), chosen_server).await;

            let mut buffer = [0; 1024];
            for _ in 0..5 {
                match self.cloud_socket.recv_from(&mut buffer).await {
                    Ok((bytes_read, src_addr)) => {
                        match serde_cbor::de::from_slice(&buffer[..bytes_read]) {
                            Ok(Msg {
                                msg_type: Type::DirOfServQueryReply(r),
                                ..
                            }) => return Some(r),
                            Ok(_) => continue,
                            Err(e) => continue,
                        }
                    }
                    Err(e) => {
                        eprintln!("Error getting the result of election: {}", e);
                        continue;
                    }
                }
            }
        }
        None
    }

    pub async fn encrypt(&mut self, pic_file_path: &str) {
        create_output_dirs();
        let pic_paths = get_pic_paths(pic_file_path);
        for pic_path in &pic_paths {
            let pic_path = Path::new(pic_path);
            let pic_with_ext = pic_path.file_name().unwrap().to_str().unwrap();
            let pic_without_ext = pic_path.file_stem().unwrap().to_str().unwrap();
            let pic_path = format!("{}/{}", HIGH_RES_PICS_PATH, pic_with_ext);
            // let pic_path = pic_path.to_str().unwrap();
            let socket = self.cloud_socket.clone();
            let id = self.get_next_req_id();

            // trigger election
            if let Some(chosen_server) = self
                .send_init_request_to_cloud(Type::ClientRequest(id))
                .await
            {
                let chosen_server = chosen_server.to_string();
                println!("Sending img to sever {}", chosen_server);

                // send the img to be encrypted
                let contents = fs::read(pic_path).await.unwrap();
                let msg_id = format!("{}:{}", socket.local_addr().unwrap(), id);
                // println!("Message Length {}", contents.len());
                fragment::client_send(contents, socket.clone(), &chosen_server, &msg_id, false)
                    .await;
                println!("Finished sending pic");

                let encoded_bytes = fragment::receive_all(socket.clone()).await;
                let encoded_image: Image = serde_cbor::de::from_slice(&encoded_bytes).unwrap();
                // mkdir(format!("{}/{}", PICS_ROOT_PATH, ENCRYPTED_PICS_PATH).as_str());
                // encoded_images.insert(msg_id, encoded_image.clone());
                let image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>> =
                    image::ImageBuffer::from_raw(
                        encoded_image.dims.0,
                        encoded_image.dims.1,
                        encoded_image.data,
                    )
                    .unwrap();

                save_image_buffer(
                    image_buffer.clone(),
                    format!("{}/{}.jpg", ENCRYPTED_PICS_PATH, pic_without_ext),
                );

                // let secret_bytes = decode_img(image_buffer).await;
                // let _ = tokio::fs::write(format!("secret_{pic_with_ext}"), secret_bytes).await;
            } else {
                println!("Failed to get the election result (server ip)");
            };
        }
    }

    pub async fn quit(&self) {
        ClientDirOfService::leave(self.cloud_socket.clone(), self.cloud_servers.clone()).await;
        std_fs::write(REQ_ID_LOG_FILEPATH, self.next_req_id.to_string())
            .expect("Failed to write req_id to req_id_log.txt");
        // complete logic for quit
    }

    fn get_next_req_id(&mut self) -> u32 {
        let id = self.next_req_id;
        self.next_req_id += 1;
        id
    }
}

fn save_image_buffer(image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>>, filename: String) {
    let image = image::DynamicImage::ImageRgba8(image_buffer);
    image.save(filename).unwrap();
}

fn file_as_image_buffer(filename: String) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let img = open(Path::new(&filename)).unwrap();
    img.to_rgba8()
}

// #[tokio::main]
// async fn main() {
//     let args: Vec<_> = env::args().collect();
//     create_output_dirs();
//     let mode = args[1].as_str();
//     let ip: SocketAddr = args[2]
//         .as_str()
//         .parse()
//         .expect("Failed to parse IP from input");

//     //Socket for communication with clients
//     let ip_client = SocketAddr::new(ip.ip(), ip.port() + 1);
//     let pic_file_path: &str = args[3].as_str();
//     let pic_paths = get_pic_paths(pic_file_path);
//     let mut id = get_req_id_log(REQ_ID_LOG_FILEPATH);
//     let socket = Arc::new(UdpSocket::bind(ip).await.expect("Failed to bind to ip"));
//     println!("Cloud Communication on {ip}");
//     let client_socket = Arc::new(
//         UdpSocket::bind(ip_client)
//             .await
//             .expect("Failed to bind to ip"),
//     );
//     println!("Clients Communication on {ip_client}");

//     let mut encoded_images: HashMap<String, Image> = HashMap::new();
//     let mut service_buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];

//     for pic_path in &pic_paths {
//         let pic_path = Path::new(pic_path);
//         let pic_with_ext = pic_path.file_name().unwrap().to_str().unwrap();
//         let pic_without_ext = pic_path.file_stem().unwrap().to_str().unwrap();
//         let pic_path = pic_path.to_str().unwrap();
//         println!("{:?} - {}", pic_path, pic_without_ext);

//         // trigger election
//         if let Some(chosen_server) = send_init_request_to_cloud(socket.clone(), id, mode).await {
//             let chosen_server = chosen_server.to_string();
//             println!("Sending img to sever {}", chosen_server);

//             // send the img to be encrypted
//             let contents = fs::read(pic_path).await.unwrap();
//             let msg_id = format!("{}:{}", socket.local_addr().unwrap(), id);
//             // println!("Message Length {}", contents.len());
//             fragment::client_send(contents, socket.clone(), &chosen_server, &msg_id).await;
//             println!("Finished sending pic");
//             let encoded_bytes = fragment::recieve(socket.clone()).await;
//             let encoded_image: Image = serde_cbor::de::from_slice(&encoded_bytes).unwrap();
//             encoded_images.insert(msg_id, encoded_image.clone());
//             let image_buffer: image::ImageBuffer<Rgba<u8>, Vec<u8>> = image::ImageBuffer::from_raw(
//                 encoded_image.dims.0,
//                 encoded_image.dims.1,
//                 encoded_image.data,
//             )
//             .unwrap();
//             save_image_buffer(
//                 image_buffer.clone(),
//                 format!("output/encoded/encoded_output_{pic_without_ext}.jpeg"),
//             );
//             let secret_bytes = decode_image(image_buffer).await;
//             let _ = tokio::fs::write(
//                 format!("output/decoded/secret_{pic_with_ext}"),
//                 secret_bytes,
//             )
//             .await;

//             id += 1;
//         } else {
//             println!("Failed to get the election result (server ip)");
//         };
//     }

//     let h1 = tokio::spawn({
//         async move {
//             loop {
//                 match client_socket.recv_from(&mut service_buffer).await {
//                     Ok((bytes_read, src_addr)) => {
//                         println!("{} bytes from {}.", bytes_read, src_addr);

//                         let msg: Msg = serde_cbor::de::from_slice(&service_buffer[..bytes_read])
//                             .expect("Failed to deserialize msg from client socket");

//                         match msg.msg_type {
//                             Type::ImageRequest(img_id, requested_access) => {
//                                 handle_image_request(
//                                     img_id.clone(),
//                                     encoded_images.get(&img_id).unwrap().clone(),
//                                     requested_access,
//                                     client_socket.clone(),
//                                     src_addr,
//                                 )
//                                 .await;
//                             }
//                             Type::ShareImage(img_id, img, recieved_access) => {
//                                 handle_shared_image(
//                                     img_id,
//                                     img,
//                                     recieved_access,
//                                     client_socket.clone(),
//                                     src_addr,
//                                 )
//                                 .await;
//                             }
//                             Type::UpdateAccess(img_id, new_access) => {
//                                 handle_update_access(
//                                     img_id.clone(),
//                                     encoded_images.get(&img_id).unwrap().clone(),
//                                     new_access,
//                                     client_socket.clone(),
//                                     src_addr,
//                                 )
//                                 .await;
//                             }
//                             _ => {}
//                         }
//                     }
//                     Err(e) => {
//                         eprintln!("Error receiving data from client socket: {}", e);
//                     }
//                 }
//             }
//         }
//     });
// }
