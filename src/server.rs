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
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::cmp::{max, min};
use std::future::Future;
use std::net::SocketAddr;
use std::process::Command;
use std::str::FromStr;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::{env, fs as std_fs};
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration};
use tokio::{net::UdpSocket, sync::Mutex};

use std::collections::HashMap;
use std::collections::HashSet;
use steganography::encoder::Encoder;
use tokio::fs;

mod fragment;
use fragment::{BigMessage, Fragment, Image, Msg, Type};

const BUFFER_SIZE: usize = 32768;
const FRAG_SIZE: usize = 4096;
const BLOCK_SIZE: usize = 2;
const TIMEOUT_MILLIS: usize = 2000;

// struct to hold server states (modify as needed)
#[derive(Clone)]
struct ServerStats {
    elections_initiated_by_me: HashSet<String>, // req_id -> (own_p, #anticipated Oks)
    elections_received_oks: HashSet<String>,    // req_id -> #currently received Oks
    running_elections: HashMap<String, u32>,
    requests_buffer: HashMap<String, Msg>,
    peer_servers: Vec<(SocketAddr, SocketAddr)>,
    sockets_ips: (String, String),
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
    p: u32,
    req_id: String,
    src_addr: std::net::SocketAddr,
    service_socket: Arc<UdpSocket>,
    election_socket: Arc<UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
) {
    let mut data = stats.lock().await;
    let own_priority = data
        .running_elections
        .entry(req_id.clone())
        .or_insert_with(|| {
            let mut rng = rand::thread_rng();
            rng.gen_range(1..1000) as u32
        })
        .to_owned();
    println!("[{}] Own priority {}", req_id, own_priority);

    if own_priority > p {
        println!("[{}] Own priority is higher", req_id);
        drop(data);
        send_ok_msg(
            req_id.clone(),
            src_addr,
            election_socket.clone(),
            stats.clone(),
        )
        .await;

        if !stats
            .lock()
            .await
            .elections_initiated_by_me
            .contains(&req_id)
        {
            send_election_msg(
                service_socket.clone(),
                election_socket.clone(),
                stats.clone(),
                req_id.clone(),
                false,
            )
            .await;
        }
    }
}

async fn reply_to_client(socket: Arc<UdpSocket>, req_id: String, stats: Arc<Mutex<ServerStats>>) {
    let data = stats.lock().await;
    // let target_addr = data.requests_buffer.get(&req_id).unwrap().sender;
    match data.requests_buffer.get(&req_id) {
        Some(s) => {
            let s = s.to_owned();
            println!("[{}] Replying to Client", req_id);
            let addr = &socket.local_addr().unwrap().to_string();
            let target_addr = s.sender;
            let response = "chosen server";
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
    peer_servers: Vec<(SocketAddr, SocketAddr)>,
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
    if !init_f || !data.running_elections.contains_key(&req_id) {
        let sender = election_socket.local_addr().unwrap();
        println!("[{}] Sending Election msgs! - {}", req_id, init_f);

        let peer_servers = data.get_peer_servers();

        let own_priority = data
            .running_elections
            .entry(req_id.clone())
            .or_insert_with(|| {
                let mut rng = rand::thread_rng();
                rng.gen_range(1..1000) as u32
            })
            .to_owned();

        if !data.elections_initiated_by_me.contains(&req_id) {
            data.elections_initiated_by_me.insert(req_id.clone());
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
        let sleep = sleep(Duration::from_millis(500));
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
    src_addr: SocketAddr,
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
                src_addr,
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
        Type::CoordinatorBrdCast(coordinator_ip) => {
            let req_id = msg.payload.clone().unwrap();
            println!("[{}] Handlign Broadcast!", req_id);
            handle_coordinator(stats.to_owned(), req_id).await;
        }
        Type::OKMsg(priority) => {
            let req_id = msg.payload.clone().unwrap();
            println!("[{}] Handling OK", req_id);
            handle_ok_msg(req_id, stats.to_owned()).await;
        }
        _ => {}
    }
}

fn get_peer_servers(filepath: &str, own_ip: SocketAddr) -> Vec<(SocketAddr, SocketAddr)> {
    let mut servers = Vec::<(SocketAddr, SocketAddr)>::new();
    let contents =
        std_fs::read_to_string(filepath).expect("Should have been able to read the file");
    for ip in contents.lines() {
        if ip != own_ip.to_string() {
            let election_ip: SocketAddr = ip.parse().unwrap();
            let service_ip = SocketAddr::new(election_ip.ip(), election_ip.port() - 1);
            servers.push((service_ip, election_ip));
        }
    }
    servers
}

async fn handle_fragmenets(
    socket: Arc<UdpSocket>,
    frag: Fragment,
    src_addr: std::net::SocketAddr,
    map: &mut HashMap<String, BigMessage>,
) -> Option<String> {
    println!("Received fragment {}.", frag.frag_id);

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

        e.insert(msg);
    } else {
        // should not be needed since we know key exists
        let _default_msg = BigMessage::default_msg();
        let big_msg = map.entry(frag.msg_id.clone()).or_insert(_default_msg);

        if !(big_msg.received_frags.contains(&frag.frag_id)) {
            big_msg.data[st_idx..end_idx].copy_from_slice(&frag.data);

            big_msg.received_len += (end_idx - st_idx) as u32;
            big_msg.received_frags.insert(frag.frag_id);
        }

        if big_msg.received_len == big_msg.msg_len
            || (big_msg.received_len as usize / FRAG_SIZE) % BLOCK_SIZE == 0
        {
            // send ack
            let block_id = (big_msg.received_len as usize + FRAG_SIZE * BLOCK_SIZE - 1)
                / (FRAG_SIZE * BLOCK_SIZE)
                - 1;
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
        if big_msg.received_len == big_msg.msg_len {
            println!("Full message is received!");
            let big_msg = big_msg.to_owned();
            return Some(frag.msg_id);
        }
    }
    None
}

#[tokio::main]
async fn main() {
    let mut stats = ServerStats::new();
    let args: Vec<_> = env::args().collect();

    let ip_elec: SocketAddr = args[1].as_str().parse().unwrap();
    let ip_service = SocketAddr::new(ip_elec.ip(), ip_elec.port() - 1);

    let peer_servers_filepath: &str = "./servers.txt";
    stats.peer_servers = get_peer_servers(peer_servers_filepath, ip_elec);
    stats.sockets_ips = (ip_service.to_string(), ip_elec.to_string());

    let service_socket = UdpSocket::bind(ip_service).await.unwrap();
    println!("Server (service) listening on {ip_service}");
    let service_socket = Arc::new(service_socket);
    let service_socket2 = Arc::clone(&service_socket);

    let election_socket = UdpSocket::bind(ip_elec).await.unwrap();
    println!("Server (election) listening on {ip_elec}");
    let election_socket = Arc::new(election_socket);
    let election_socket2 = Arc::clone(&election_socket);

    let stats = Arc::new(Mutex::new(stats));
    let stats_service = Arc::clone(&stats);
    let stats_election = Arc::clone(&stats);

    let mut service_buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
    let mut election_buffer: [u8; 2048] = [0; 2048];

    let mut map: HashMap<String, BigMessage> = HashMap::new();
    let mut channels_map: HashMap<String, mpsc::Sender<u32>> = HashMap::new();

    let h1 = tokio::spawn({
        async move {
            loop {
                match service_socket.recv_from(&mut service_buffer).await {
                    Ok((bytes_read, src_addr)) => {
                        println!("{} bytes from {}.", bytes_read, src_addr);

                        let msg: Msg =
                            serde_cbor::de::from_slice(&service_buffer[..bytes_read]).unwrap();

                        match msg.msg_type {
                            Type::Fragment(frag) => {
                                let service_socket = Arc::clone(&service_socket);
                                if let Some(req_id) = handle_fragmenets(
                                    service_socket.clone(),
                                    frag,
                                    src_addr,
                                    &mut map,
                                )
                                .await
                                {
                                    let data = map.get(&req_id).unwrap().data.to_owned();
                                    let (tx, rx) = mpsc::channel(100);
                                    channels_map.insert(req_id.clone(), tx);

                                    let h = tokio::spawn(async move {
                                        let default_image =
                                            image::open("default_image.png").unwrap();
                                        let encoder = Encoder::new(&data, default_image);
                                        let encoded_image = encoder.encode_alpha();
                                        let image = Image {
                                            dims: encoded_image.dimensions(),
                                            data: encoded_image.into_raw(),
                                        };

                                        let encoded_bytes = serde_cbor::to_vec(&image).unwrap();
                                        fragment::server_send(
                                            encoded_bytes,
                                            service_socket.clone(),
                                            src_addr.to_string().as_str(),
                                            &req_id,
                                            rx,
                                        )
                                        .await;
                                    });
                                }
                            }
                            Type::Ack(msg_id, block_id) => {
                                println!("ACK: {}", msg_id);
                                channels_map
                                    .get(&msg_id)
                                    .unwrap()
                                    .send(block_id)
                                    .await
                                    .unwrap()
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
            sockets_ips: (String::new(), String::new()),
        }
    }

    fn get_peer_servers(&self) -> Vec<(SocketAddr, SocketAddr)> {
        self.peer_servers.clone()
    }
}
