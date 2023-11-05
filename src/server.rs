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
use std::cmp::max;
use std::net::SocketAddr;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use std::{env, fs};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration};
use tokio::{net::UdpSocket, sync::Mutex};

use std::collections::HashMap;
use std::collections::HashSet;

// struct to hold server states (modify as needed)
#[derive(Clone)]
struct ServerStats {
    elections_initiated_by_me: HashMap<String, (u32, u32)>, // req_id -> (own_p, #anticipated Oks)
    elections_received_oks: HashMap<String, HashMap<String, u32>>, // req_id -> #currently received Oks
    elections_leader: HashMap<String, (String, u32)>, // req_id -> (addr of currently chosen leader, its priority)
    running_elections: HashMap<String, u32>,
    requests_buffer: HashMap<String, Msg>,
    peer_servers: Vec<(SocketAddr, SocketAddr)>,
    sockets_ips: (String, String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum Type {
    ClientRequest(u32),
    ElectionRequest(u32),
    OKMsg(u32),
    CoordinatorBrdCast(String),
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Msg {
    sender: SocketAddr,
    receiver: SocketAddr,
    msg_type: Type,
    payload: Option<String>,
}

async fn handle_ok_msg(
    req_id: String,
    src_addr: SocketAddr,
    p: u32,
    socket: Arc<&UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
) {
    let mut data = stats.lock().await;
    data.elections_received_oks
        .entry(req_id.clone())
        .or_insert_with(HashMap::new)
        .insert(src_addr.to_string(), p);
}

async fn send_ok_msg(
    req_id: String,
    src_addr: std::net::SocketAddr,
    socket: Arc<&UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
) {
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
    socket: Arc<&UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
) {
    let mut data = stats.lock().await;
    if !data.running_elections.contains_key(&req_id) {
        let own_priority = {
            let mut rng = rand::thread_rng();
            rng.gen_range(1..1000)
        } as u32;
        data.running_elections.insert(req_id.clone(), own_priority);
    }
    if *data.running_elections.get(&req_id).unwrap() > p {
        drop(data);
        send_ok_msg(req_id, src_addr, socket, stats).await;
    }
}

async fn reply_to_client(socket: Arc<&UdpSocket>, req_id: String, stats: Arc<Mutex<ServerStats>>) {
    let data = stats.lock().await;
    println!("Replying to Client");
    let addr = &socket.local_addr().unwrap().to_string();
    let response = format!("Hello! This is SERVER {} for req_id {}\n", addr, req_id);
    let target_addr = data.requests_buffer.get(&req_id).unwrap().sender;
    socket
        .send_to(response.as_bytes(), target_addr)
        .await
        .unwrap();
}

async fn handle_coordinator(
    stats: Arc<Mutex<ServerStats>>,
    coordinator_ip: String,
    socket: Arc<&UdpSocket>,
    req_id: String,
) {
    let mut data = stats.lock().await;
    let ips = data.sockets_ips.clone();
    data.running_elections
        .remove(&req_id)
        .expect("Attempt to remove req_id that does not exist");
    if coordinator_ip == ips.0 || coordinator_ip == ips.1 {
        drop(data);
        reply_to_client(socket, req_id, stats).await;
    }
}

async fn broadcast_coordinator(
    socket: Arc<&UdpSocket>,
    leader: String,
    peer_servers: Vec<(SocketAddr, SocketAddr)>,
    req_id: String,
) {
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

async fn initiate_election(
    socket: Arc<&UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
    req_id: String,
) {
    let mut data = stats.lock().await;

    let sender = socket.local_addr().unwrap();
    println!("{} Initiating Election!", sender);

    let peer_servers = data.get_peer_servers();
    println!("Inserting Request {} in running elections", req_id);
    let own_priority = {
        let mut rng = rand::thread_rng();
        rng.gen_range(1..1000)
    } as u32;
    println!("My own Priority {}", own_priority);

    data.running_elections.insert(req_id.clone(), own_priority);
    println!("Inserting Request {} in initiated elections", req_id);

    for server in &peer_servers {
        let msg = Msg {
            sender,
            receiver: server.1,
            msg_type: Type::ElectionRequest(own_priority),
            payload: Some(req_id.clone()),
        };
        let serialized_msg = serde_json::to_string(&msg).unwrap();
        socket
            .send_to(serialized_msg.as_bytes(), server.1)
            .await
            .unwrap();
    }

    sleep(Duration::from_millis(20)).await;

    let (coordinator_id, max_priority) = data
        .elections_received_oks
        .get(&req_id)
        .and_then(|inner_map| inner_map.iter().max_by_key(|&(_, p)| p))
        .map(|(ip, p)| (ip.to_owned(), *p))
        .unwrap_or((sender.to_string(), own_priority));

    broadcast_coordinator(socket, coordinator_id, peer_servers, req_id).await;
}

async fn handle_client(
    id: u32,
    msg: Msg,
    socket: Arc<&UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
    src_addr: SocketAddr,
) {
    let req_id = format!("{}:{}", msg.sender, id);
    println!("Handling Request {req_id}");
    let mut data = stats.lock().await;
    data.requests_buffer
        .entry(req_id.clone())
        .or_insert_with(|| msg.clone());

    if data.running_elections.contains_key(&req_id) {
        println!("Election for this request is initialized, Buffering request!");
    } else {
        println!("Buffering {req_id}");
        let random = {
            let mut rng = rand::thread_rng();
            rng.gen_range(10..30)
        } as u64;
        sleep(Duration::from_millis(random)).await;

        if !data.running_elections.contains_key(&req_id) {
            drop(data);
            initiate_election(socket, stats, req_id).await;
        }
    }
}

async fn handle_request(
    buffer: &[u8],
    src_addr: std::net::SocketAddr,
    socket: Arc<&UdpSocket>,
    stats: &Arc<Mutex<ServerStats>>,
) {
    let request: &str = std::str::from_utf8(buffer).expect("Failed to convert to UTF-8");
    let msg: Msg = match serde_json::from_str(request) {
        Ok(msg) => msg,
        Err(_) => return,
    };
    println!(" Received message :\n{}", request);

    match msg.msg_type {
        Type::ClientRequest(req_id) => {
            handle_client(req_id, msg, socket, stats.to_owned(), src_addr).await;
        }
        Type::ElectionRequest(priority) => {
            println!("handling election!");
            let req_id = msg.payload.clone().unwrap();
            handle_election(priority, req_id, src_addr, socket, stats.to_owned()).await;
        }
        Type::CoordinatorBrdCast(coordinator_ip) => {
            println!("Handlign Broadcast!");
            let req_id = msg.payload.clone().unwrap();
            handle_coordinator(stats.to_owned(), coordinator_ip, socket, req_id).await;
        }
        Type::OKMsg(priority) => {
            println!("Handling OK");
            let req_id = msg.payload.clone().unwrap();
            handle_ok_msg(req_id, src_addr, priority, socket, stats.to_owned()).await;
        }
    }
}

fn get_peer_servers(filepath: &str, own_ip: SocketAddr) -> Vec<(SocketAddr, SocketAddr)> {
    let mut servers = Vec::<(SocketAddr, SocketAddr)>::new();
    let contents = fs::read_to_string(filepath).expect("Should have been able to read the file");
    for ip in contents.lines() {
        if ip != own_ip.to_string() {
            let service_ip: SocketAddr = ip.parse().unwrap();
            let election_ip = SocketAddr::new(service_ip.ip(), service_ip.port() + 1);
            servers.push((service_ip, election_ip));
        }
    }
    servers
}

#[tokio::main]
async fn main() {
    let mut stats = ServerStats::new();
    let args: Vec<_> = env::args().collect();

    let ip_service: SocketAddr = args[1].as_str().parse().unwrap();
    let ip_elec = SocketAddr::new(ip_service.ip(), ip_service.port() + 1);

    let peer_servers_filepath: &str = "./servers.txt";
    stats.peer_servers = get_peer_servers(peer_servers_filepath, ip_service);
    stats.sockets_ips = (ip_service.to_string(), ip_elec.to_string());

    let service_socket = UdpSocket::bind(ip_service).await.unwrap();
    println!("Server (service) listening on {ip_service}");

    let election_socket = UdpSocket::bind(ip_elec).await.unwrap();
    println!("Server (election) listening on {ip_elec}");

    let stats = Arc::new(Mutex::new(stats));
    let mut election_buffer = [0; 2048];
    let mut service_buffer = [0; 2048];
    let stats_service = Arc::clone(&stats);
    let stats_election = Arc::clone(&stats);

    let h1 = tokio::spawn({
        async move {
            loop {
                match service_socket.recv_from(&mut service_buffer).await {
                    Ok((bytes_read, src_addr)) => {
                        let socket_clone = Arc::clone(&Arc::new(&service_socket));
                        handle_request(
                            &service_buffer[..bytes_read],
                            src_addr,
                            socket_clone,
                            &stats_service,
                        )
                        .await;
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
                        let socket_clone = Arc::clone(&Arc::new(&election_socket));
                        handle_request(
                            &election_buffer[..bytes_read],
                            src_addr,
                            socket_clone,
                            &stats_election,
                        )
                        .await;
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
            elections_initiated_by_me: HashMap::new(),
            elections_leader: HashMap::new(),
            elections_received_oks: HashMap::new(),
            running_elections: HashMap::new(),
            peer_servers: Vec::new(),
            sockets_ips: (String::new(), String::new()),
        }
    }

    fn get_peer_servers(&self) -> Vec<(SocketAddr, SocketAddr)> {
        self.peer_servers.clone()
    }
}
