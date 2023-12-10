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
use image::DynamicImage;
use rand::Rng;
use std::cmp::min;
use std::net::SocketAddr;
use std::time::Instant;

use std::ops::Index;
use std::sync::Arc;
use std::{env, fs as std_fs};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio::{net::UdpSocket, sync::Mutex};

use image::{ImageBuffer, Rgba};
use std::collections::HashMap;
use std::collections::HashSet;
use std::thread::sleep as std_sleep;
use std::time::Duration as std_duration;
use steganography::encoder::Encoder;
use sysinfo::{CpuExt, CpuRefreshKind, RefreshKind, System, SystemExt};

mod fragment;
use fragment::{BigMessage, Fragment, Image, Msg, Type};
use fragment::{BLOCK_SIZE, BUFFER_SIZE, FRAG_SIZE};
use fragment::{ELECTION_PORT, SERVERS_FILEPATH, SERVICE_PORT, SERVICE_SENDBACK_PORT};

// struct to hold server states (modify as needed)
#[derive(Clone)]
struct ServerStats {
    elections_initiated_by_me: HashSet<String>, // req_id -> (own_p, #anticipated Oks)
    elections_received_oks: HashSet<String>,    // req_id -> #currently received Oks
    running_elections: HashMap<String, f32>,
    requests_buffer: HashMap<String, Msg>,
    peer_servers: Vec<(SocketAddr, SocketAddr, SocketAddr)>,
    own_ips: Option<(SocketAddr, SocketAddr, SocketAddr)>,
    down: bool,
}

async fn handle_ok_msg(req_id: String, stats: Arc<Mutex<ServerStats>>) {
    let mut data = stats.lock().await;
    data.elections_received_oks.insert(req_id);
}

async fn send_ok_msg(
    req_id: String,
    src_addr: std::net::SocketAddr,
    socket: Arc<UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
) {
    println!("[{}] sending ok msg", req_id);
    let data = stats.lock().await;
    let own_priority = *data.running_elections.get(&req_id).unwrap();
    let msg = Msg {
        sender: socket.local_addr().unwrap(),
        receiver: src_addr,
        msg_type: Type::OKMsg(own_priority),
        payload: Some(req_id.clone()),
    };
    let serialized_msg = serde_json::to_string(&msg).unwrap();
    socket
        .send_to(serialized_msg.as_bytes(), src_addr)
        .await
        .unwrap();
}

async fn handle_election(
    p: f32,
    req_id: String,
    src_addr: std::net::SocketAddr,
    service_socket: Arc<UdpSocket>,
    election_socket: Arc<UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
) {
    // let mut sys = System::new_with_specifics(
    //     RefreshKind::new()
    //         .with_cpu(CpuRefreshKind::everything())
    //         .with_memory(),
    // );
    // sys.refresh_cpu();
    // let priority = 16.0 - sys.load_average().one as f32;
    let random = {
        let mut rng = rand::thread_rng();
        rng.gen_range(10..30)
    } as u64;
    let other_server_port = src_addr.port();
    let own_port = election_socket.local_addr().unwrap().port();
    let mut data = stats.lock().await;
    let own_priority = data
        .running_elections
        .entry(req_id.clone())
        .or_insert(random as f32)
        .to_owned();
    println!("[{}] Own priority {}", req_id, own_priority);

    if own_priority > p || (own_priority == p && own_port > other_server_port) {
        println!("[{}] Own priority is higher", req_id);
        drop(data);
        send_ok_msg(
            req_id.clone(),
            src_addr,
            election_socket.clone(),
            stats.clone(),
        )
        .await;

        let mut data = stats.lock().await;
        if !data.elections_initiated_by_me.contains(&req_id) {
            println!("checked for {} in elec_init_by_me", req_id);
            data.elections_initiated_by_me.insert(req_id.clone());
            println!("Added {} into elec_init_by_me", req_id);
            drop(data);
            send_election_msg(
                service_socket.clone(),
                election_socket.clone(),
                stats.clone(),
                req_id.clone(),
                false,
            )
            .await;
        }
    }else {
        drop(data);
        handle_coordinator(stats, req_id).await;
    }
}

async fn reply_to_client(socket: Arc<UdpSocket>, req_id: String, stats: Arc<Mutex<ServerStats>>) {
    let data = stats.lock().await;
    // let target_addr = data.requests_buffer.get(&req_id).unwrap().sender;
    match data.requests_buffer.get(&req_id) {
        Some(s) => {
            let s = s.to_owned();
            println!("[{}] Replying to Client", req_id);
            let target_addr = s.sender;
            let response = data.own_ips.unwrap().0.to_string();
            println!("{}", response);
            socket
                .send_to(response.as_bytes(), target_addr)
                .await
                .unwrap();
        }
        None => {
            println!("[{}] Aborting replying to client", req_id);
        }
    };
}

async fn handle_coordinator(stats: Arc<Mutex<ServerStats>>, req_id: String) {
    println!("[{}] Flushing related stats", req_id);
    let mut data = stats.lock().await;
    if data.running_elections.remove(&req_id).is_some() {
        // Entry was removed (if it existed)
    }

    if data.elections_received_oks.remove(&req_id) {
        // Entry was removed (if it existed)
    }

    if data.elections_initiated_by_me.remove(&req_id) {
        // Entry was removed (if it existed)
    }

    if data.requests_buffer.remove(&req_id).is_some() {
        // Entry was removed (if it existed)
    }
}

async fn broadcast_coordinator(
    socket: Arc<UdpSocket>,
    leader: String,
    peer_servers: Vec<(SocketAddr, SocketAddr, SocketAddr)>,
    req_id: String,
) {
    println!("[{}] broadcasting as a coordinator", req_id);
    for server in &peer_servers {
        let msg = Msg {
            sender: socket.local_addr().unwrap(),
            receiver: server.1,
            msg_type: Type::CoordinatorBrdCast(leader.clone()),
            payload: Some(req_id.clone()),
        };
        let serialized_msg = serde_json::to_string(&msg).unwrap();
        socket
            .send_to(serialized_msg.as_bytes(), server.1)
            .await
            .unwrap();
    }
}

async fn send_election_msg(
    service_socket: Arc<UdpSocket>,
    election_socket: Arc<UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
    req_id: String,
    init_f: bool,
) {
    let mut data = stats.lock().await;
    if !init_f
        || (!data.running_elections.contains_key(&req_id)
            && !data.elections_initiated_by_me.contains(&req_id))
    {
        let sender = election_socket.local_addr().unwrap();
        println!("[{}] Sending Election msgs! - {}", req_id, init_f);

        let peer_servers = data.get_peer_servers();
        // let mut sys = System::new_with_specifics(
        //     RefreshKind::new()
        //         .with_cpu(CpuRefreshKind::everything())
        //         .with_memory(),
        // );
        // sys.refresh_cpu();
        // let priority = 16.0 - sys.load_average().one as f32;
        let random = {
            let mut rng = rand::thread_rng();
            rng.gen_range(10..30)
        } as u64;
        let own_priority = data
            .running_elections
            .entry(req_id.clone())
            .or_insert(random as f32)
            .to_owned();

        if !data.elections_initiated_by_me.contains(&req_id) {
            data.elections_initiated_by_me.insert(req_id.clone());
            println!("Added {} into elec_init_by_me", req_id);
        }
        drop(data);
        println!("[{}] My own Priority {} - {}", req_id, own_priority, init_f);

        for server in &peer_servers {
            let msg = Msg {
                sender,
                receiver: server.1,
                msg_type: Type::ElectionRequest(own_priority),
                payload: Some(req_id.clone()),
            };
            let serialized_msg = serde_json::to_string(&msg).unwrap();
            election_socket
                .send_to(serialized_msg.as_bytes(), server.1)
                .await
                .unwrap();
        }
        println!("[{}] Waiting for ok msg - {}", req_id, init_f);
        let sleep = sleep(Duration::from_millis(700));
        tokio::pin!(sleep);
        // sleep(Duration::from_millis(1000)).await;

        tokio::select! {
            _ = &mut sleep => {
                // Code to execute when sleep completes
                println!("Sleep completed");
            },
            _ = check_for_oks(stats.clone(), req_id.clone()) => {
                // Code to execute when check_for_oks completes
                println!("check_for_oks completed");
            }
        }

        let data = stats.lock().await;

        if !data.elections_received_oks.contains(&req_id) {
            println!("[{}] Did not find ok msgs - {}", req_id, init_f);
            drop(data);
            let own_ip = election_socket.local_addr().unwrap().to_string();
            broadcast_coordinator(
                election_socket.clone(),
                own_ip,
                peer_servers,
                req_id.clone(),
            )
            .await;
            reply_to_client(service_socket.clone(), req_id.clone(), stats.clone()).await;
            handle_coordinator(stats.clone(), req_id.clone()).await;
        }
    }
}

async fn check_for_oks(stats: Arc<Mutex<ServerStats>>, req_id: String) {
    loop {
        match stats.lock().await.elections_received_oks.contains(&req_id) {
            true => break,
            false => continue,
        }
    }
}

async fn handle_client(
    id: u32,
    msg: Msg,
    service_socket: Arc<UdpSocket>,
    election_socket: Arc<UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
) {
    let req_id = format!("{}:{}", msg.sender, id);
    println!("[{}] Handling client Request", req_id);
    let mut data = stats.lock().await;
    data.requests_buffer
        .entry(req_id.clone())
        .or_insert_with(|| msg.clone());

    if data.running_elections.contains_key(&req_id) {
        println!(
            "[{}] Election for this request is initialized, Buffering request!",
            req_id
        );
    } else {
        drop(data);
        println!("[{}] Buffering", req_id);
        let random = {
            let mut rng = rand::thread_rng();
            rng.gen_range(10..30)
        } as u64;
        sleep(Duration::from_millis(random)).await;

        let data = stats.lock().await;
        if !data.running_elections.contains_key(&req_id) {
            drop(data);
            send_election_msg(service_socket, election_socket, stats, req_id, true).await;
        }
    }
}

async fn handle_elec_request(
    buffer: &[u8],
    src_addr: std::net::SocketAddr,
    service_socket: Arc<UdpSocket>,
    election_socket: Arc<UdpSocket>,
    stats: &Arc<Mutex<ServerStats>>,
) {
    let request: &str = std::str::from_utf8(buffer).expect("Failed to convert to UTF-8");
    let msg: Msg = match serde_json::from_str(request) {
        Ok(msg) => msg,
        Err(_) => return,
    };
    println!("Received message :\n{}", request);

    match msg.msg_type {
        Type::ClientRequest(req_id) => {
            handle_client(
                req_id,
                msg,
                service_socket,
                election_socket,
                stats.to_owned(),
            )
            .await;
        }
        Type::ElectionRequest(priority) => {
            let req_id = msg.payload.clone().unwrap();
            println!("[{}] handling election!", req_id);
            handle_election(
                priority,
                req_id,
                src_addr,
                service_socket,
                election_socket,
                stats.to_owned(),
            )
            .await;
        }
        Type::CoordinatorBrdCast(_coordinator_ip) => {
            let req_id = msg.payload.clone().unwrap();
            println!("[{}] Handlign Broadcast!", req_id);
            handle_coordinator(stats.to_owned(), req_id).await;
        }
        Type::OKMsg(_priority) => {
            let req_id = msg.payload.clone().unwrap();
            println!("[{}] Handling OK", req_id);
            handle_ok_msg(req_id, stats.to_owned()).await;
        }
        Type::Fail(fail_time) => {
            println!("Will fail for {}", fail_time);
            handle_fail_msg(fail_time, election_socket.clone(), &stats.clone()).await;
        }
        _ => {}
    }
}

async fn handle_fragmenets(
    socket: Arc<UdpSocket>,
    frag: Fragment,
    src_addr: std::net::SocketAddr,
    map: &mut HashMap<String, BigMessage>,
) -> Option<String> {
    println!("[{}] Received fragment {}.", frag.msg_id, frag.frag_id);
    let mut new_frag = false;

    let st_idx = FRAG_SIZE * (frag.frag_id as usize);
    let end_idx = min(
        FRAG_SIZE * (frag.frag_id + 1) as usize,
        frag.msg_len as usize,
    );

    // if this is the first fragment create a new entry in the map
    if let std::collections::hash_map::Entry::Vacant(e) = map.entry(frag.msg_id.clone()) {
        let mut msg_data = vec![0; frag.msg_len as usize];
        println!("{}", frag.msg_id);
        msg_data[st_idx..end_idx].copy_from_slice(&frag.data);

        let mut received_frags = HashSet::new();
        received_frags.insert(frag.frag_id);

        let msg = BigMessage {
            data: msg_data,
            msg_len: frag.msg_len,
            received_len: (end_idx - st_idx) as u32,
            received_frags,
        };

        if msg.received_len == msg.msg_len
            || (msg.received_len as usize / FRAG_SIZE) % BLOCK_SIZE == 0
        {
            // send ack
            let block_id = (msg.received_len as usize + FRAG_SIZE * BLOCK_SIZE - 1)
                / (FRAG_SIZE * BLOCK_SIZE)
                - 1;
            println!("Sending ACK for block {}", block_id);
            let ack = Msg {
                msg_type: Type::Ack(frag.msg_id.clone(), block_id as u32),
                sender: socket.local_addr().unwrap(),
                receiver: src_addr,
                payload: None,
            };

            let ack = serde_cbor::ser::to_vec(&ack).unwrap();
            socket
                .send_to(&ack, src_addr.to_string())
                .await
                .expect("Failed to send!");
        }
        if msg.received_len == msg.msg_len {
            println!("Full message is received!");
            e.insert(msg);
            return Some(frag.msg_id);
        }
        e.insert(msg);
    } else {
        // should not be needed since we know key exists
        let _default_msg = BigMessage::default_msg();
        let big_msg = map.entry(frag.msg_id.clone()).or_insert(_default_msg);

        if !(big_msg.received_frags.contains(&frag.frag_id)) {
            big_msg.data[st_idx..end_idx].copy_from_slice(&frag.data);

            big_msg.received_len += (end_idx - st_idx) as u32;
            big_msg.received_frags.insert(frag.frag_id);
            new_frag = true;
        }

        if new_frag
            && (big_msg.received_len == big_msg.msg_len
                || (big_msg.received_len as usize / FRAG_SIZE) % BLOCK_SIZE == 0)
        {
            // send ack
            let block_id = (big_msg.received_len as usize + FRAG_SIZE * BLOCK_SIZE - 1)
                / (FRAG_SIZE * BLOCK_SIZE)
                - 1;
            println!("Sending ACK for block {}", block_id);
            let ack = Msg {
                msg_type: Type::Ack(frag.msg_id.clone(), block_id as u32),
                sender: socket.local_addr().unwrap(),
                receiver: src_addr,
                payload: None,
            };

            let ack = serde_cbor::ser::to_vec(&ack).unwrap();
            socket
                .send_to(&ack, src_addr.to_string())
                .await
                .expect("Failed to send!");
        }
        if new_frag && big_msg.received_len == big_msg.msg_len {
            println!("Full message is received!");
            return Some(frag.msg_id);
        }
    }
    None
}

async fn send_fail_msg(socket: Arc<UdpSocket>, stats: &Arc<Mutex<ServerStats>>) {
    let peer_servres: Vec<(SocketAddr, SocketAddr, SocketAddr)> =
        stats.lock().await.get_peer_servers();

    let next_server = peer_servres[{
        let mut rng = rand::thread_rng();
        rng.gen_range(0..peer_servres.len())
    } as usize]
        .1;
    let fail_msg = Msg {
        msg_type: Type::Fail(60),
        sender: socket.local_addr().unwrap(),
        receiver: next_server,
        payload: None,
    };

    let fail_msg = serde_json::to_string(&fail_msg).unwrap();
    socket
        .send_to(fail_msg.as_bytes(), next_server.to_string())
        .await
        .expect("Failed to send!");
}

async fn handle_fail_msg(fail_time: u32, socket: Arc<UdpSocket>, stats: &Arc<Mutex<ServerStats>>) {
    let sleep_time = Duration::from_secs(fail_time as u64);
    stats.lock().await.down = true;
    sleep(sleep_time).await;
    println!("Woke Up!");
    stats.lock().await.down = false;
    send_fail_msg(socket, stats).await;
}

async fn encode(secret_bytes: Vec<u8>, req_id: String, default_image: DynamicImage) -> Vec<u8> {
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

async fn handle_encryption(
    data: Vec<u8>,
    socket: Arc<UdpSocket>,
    src_addr: SocketAddr,
    req_id: String,
    rx: mpsc::Receiver<u32>,
    default_image: DynamicImage,
) {
    let start = Instant::now();
    let encoded_bytes = encode(data, req_id.clone(), default_image).await;
    let duration = start.elapsed().as_secs_f32();
    println!(
        "[{}] finished encryption, image size is {}, Encryption time: {}",
        req_id,
        encoded_bytes.len(),
        duration
    );
    fragment::server_send(
        encoded_bytes,
        socket.clone(),
        src_addr.to_string().as_str(),
        req_id.as_str(),
        rx,
    )
    .await;
}

async fn get_peer_servers(
    filepath: &str,
    own_ips: (SocketAddr, SocketAddr, SocketAddr),
    mode: &str,
) -> Vec<(SocketAddr, SocketAddr, SocketAddr)> {
    let mut servers = Vec::<(SocketAddr, SocketAddr, SocketAddr)>::new();
    let contents =
        std_fs::read_to_string(filepath).expect("Should have been able to read the file");
    for ip in contents.lines() {
        let (ip_service, ip_elec, ip_send) = get_ips(ip, mode).await;
        if own_ips.0 != ip_service {
            servers.push((ip_service, ip_elec, ip_send));
        }
    }
    servers
}

async fn get_ips(ip: &str, mode: &str) -> (SocketAddr, SocketAddr, SocketAddr) {
    let (ip_service, ip_elec, ip_send): (SocketAddr, SocketAddr, SocketAddr) = match mode {
        "local" => {
            // In local mode, assume the provided IP includes the port number
            let ip_elec: SocketAddr = ip.parse().expect("Failed to parse ip");
            let ip_service = SocketAddr::new(ip_elec.ip(), ip_elec.port() - 1);
            let ip_send = SocketAddr::new(ip_elec.ip(), ip_elec.port() + 1);
            (ip_service, ip_elec, ip_send)
        }
        "dist" => {
            // In distributed mode, no port number, defined by CONSTS
            let ip_service: SocketAddr = format!("{}:{}", ip, SERVICE_PORT)
                .parse()
                .expect("Failed to parse ip");
            let ip_elec: SocketAddr = format!("{}:{}", ip, ELECTION_PORT)
                .parse()
                .expect("Failed to parse ip");
            let ip_send: SocketAddr = format!("{}:{}", ip, SERVICE_SENDBACK_PORT)
                .parse()
                .expect("Failed to parse ip");
            (ip_service, ip_elec, ip_send)
        }
        _ => {
            eprintln!("Invalid mode. Use 'local' or 'distributed'.");
            std::process::exit(1);
        }
    };
    (ip_service, ip_elec, ip_send)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let mut stats = ServerStats::new();
    let args: Vec<_> = env::args().collect();
    let mode = args[1].as_str();
    let ip = args[2].as_str();
    let (ip_service, ip_elec, ip_send) = get_ips(ip, mode).await;

    let init_fail: bool = args.len() == 4;
    stats.own_ips = Some((ip_service, ip_elec, ip_send));
    stats.peer_servers = get_peer_servers(SERVERS_FILEPATH, stats.own_ips.unwrap(), mode).await;
    println!("{:?}", stats.peer_servers);

    let service_socket = Arc::new(UdpSocket::bind(ip_service).await.unwrap());
    let service_socket2 = Arc::clone(&service_socket);
    println!("Server (service) listening on {ip_service}");

    let election_socket = Arc::new(UdpSocket::bind(ip_elec).await.unwrap());
    println!("Server (election) listening on {ip_elec}");

    let send_socket = Arc::new(UdpSocket::bind(ip_send).await.unwrap());
    println!("Server (send back) on {ip_send}");

    let stats = Arc::new(Mutex::new(stats));
    let stats_election = Arc::clone(&stats);

    let mut service_buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
    let mut election_buffer: [u8; 2048] = [0; 2048];

    let mut received_complete_msgs: HashMap<String, BigMessage> = HashMap::new();
    let mut channels_map: HashMap<String, mpsc::Sender<u32>> = HashMap::new();

    //Different def images with different sizes to accomodate different size of secret images
    let def1: DynamicImage = image::open("default_images/def1.png").unwrap();
    let def2: DynamicImage = image::open("default_images/def2.png").unwrap();
    let def3: DynamicImage = image::open("default_images/def3.png").unwrap();
    let def4: DynamicImage = image::open("default_images/def4.png").unwrap();
    let def5: DynamicImage = image::open("default_images/def5.png").unwrap();
    let def6: DynamicImage = image::open("default_images/def6.png").unwrap();
    println!("Loaded default images");

    if init_fail {
        send_fail_msg(election_socket.clone(), &stats).await;
    }

    let h1 = tokio::spawn({
        async move {
            loop {
                match service_socket.recv_from(&mut service_buffer).await {
                    Ok((bytes_read, src_addr)) => {
                        // println!("{} bytes from {}.", bytes_read, src_addr);

                        let msg: Msg = serde_cbor::de::from_slice(&service_buffer[..bytes_read])
                            .expect("Failed to deserialize msg from service socket");

                        match msg.msg_type {
                            Type::Fragment(frag) => {
                                let service_socket = service_socket.clone();
                                let send_socket = send_socket.clone();

                                if let Some(req_id) = handle_fragmenets(
                                    service_socket.clone(),
                                    frag,
                                    src_addr,
                                    &mut received_complete_msgs,
                                )
                                .await
                                {
                                    let data = received_complete_msgs
                                        .get(&req_id)
                                        .unwrap()
                                        .data
                                        .to_owned();
                                    received_complete_msgs.remove(&req_id);
                                    let (tx, rx) = mpsc::channel(100);
                                    channels_map.insert(req_id.clone(), tx);

                                    println!("{}", data.len());

                                    let default_image = match data.len() {
                                        len if len > 9100000 => def6.clone(),
                                        len if len > 3000000 => def5.clone(),
                                        len if len > 2100000 => def4.clone(),
                                        len if len > 670000 => def3.clone(),
                                        len if len > 350000 => def2.clone(),
                                        _ => def1.clone(),
                                    };

                                    tokio::spawn(async move {
                                        handle_encryption(
                                            data,
                                            send_socket,
                                            src_addr,
                                            req_id.clone(),
                                            rx,
                                            default_image.clone(),
                                        )
                                        .await
                                    });
                                }
                            }
                            Type::Ack(msg_id, block_id) => {
                                println!("ACK: {}", msg_id);
                                match channels_map.get(&msg_id) {
                                    Some(tx) => match tx.send(block_id).await {
                                        Ok(o) => {}
                                        Err(e) => {
                                            println!("[{}] TX Dropped!", msg_id);
                                            std::process::exit(1);
                                        }
                                    },
                                    None => {
                                        println!("TX not in the map!");
                                        std::process::exit(1);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        eprintln!("Error receiving data: {}", e);
                    }
                }
            }
        }
    });

    let h2 = tokio::spawn({
        async move {
            loop {
                match election_socket.recv_from(&mut election_buffer).await {
                    Ok((bytes_read, src_addr)) => {
                        if stats.lock().await.down
                            && stats.lock().await.running_elections.is_empty()
                        {
                            continue;
                        }
                        let service_socket = Arc::clone(&service_socket2);
                        let election_socket = Arc::clone(&election_socket);
                        let stats_clone = Arc::clone(&stats_election);
                        tokio::spawn(async move {
                            handle_elec_request(
                                &election_buffer[..bytes_read],
                                src_addr,
                                service_socket,
                                election_socket,
                                &stats_clone,
                            )
                            .await;
                        });
                    }
                    Err(e) => {
                        eprintln!("Error receiving data: {}", e);
                    }
                }
            }
        }
    });

    h1.await.unwrap();
    h2.await.unwrap();
}

impl ServerStats {
    fn new() -> ServerStats {
        ServerStats {
            requests_buffer: HashMap::new(),
            elections_initiated_by_me: HashSet::new(),
            elections_received_oks: HashSet::new(),
            running_elections: HashMap::new(),
            peer_servers: Vec::new(),
            own_ips: None,
            down: false,
        }
    }

    fn get_peer_servers(&self) -> Vec<(SocketAddr, SocketAddr, SocketAddr)> {
        self.peer_servers.clone()
    }
}
