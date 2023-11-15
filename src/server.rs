#![allow(
    dead_code,
    unused_variables,
    unused_imports,
    clippy::redundant_allocation,
    unused_assignments
)]

extern crate serde;
extern crate serde_derive;
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
    elections_initiated_by_me: HashSet<String>, // req_id -> (own_p, #anticipated Oks)
    elections_received_oks: HashSet<String>,    // req_id -> #currently received Oks
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
    println!("sending ok msg");
    let data = stats.lock().await;
    let own_priority = *data.running_elections.get(&req_id).unwrap();
    let msg = Msg {
        sender: socket.local_addr().unwrap(),
        receiver: src_addr,
        msg_type: Type::OKMsg(own_priority),
        payload: Some(req_id.clone()),
    };
    let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
    socket
        .send_to(&serialized_msg, src_addr)
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
        });
    println!("[{}] Own priority {}", req_id, own_priority);
    if *own_priority > p && !data.elections_initiated_by_me.contains(&req_id){
        println!("[{}] Own priority is higher", req_id);
        drop(data);
        send_ok_msg(
            req_id.clone(),
            src_addr,
            election_socket.clone(),
            stats.clone(),
        )
        .await;
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

async fn reply_to_client(socket: Arc<UdpSocket>, req_id: String, stats: Arc<Mutex<ServerStats>>) {
    let data = stats.lock().await;
    println!("[{}] Replying to Client", req_id);
    let addr = &socket.local_addr().unwrap().to_string();
    let response = format!("Hello! This is SERVER {} for req_id {}\n", addr, req_id);
    let target_addr = data.requests_buffer.get(&req_id).unwrap().sender;
    socket
        .send_to(response.as_bytes(), target_addr)
        .await
        .unwrap();
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
        let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
        socket
            .send_to(&serialized_msg, server.1)
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
            });
        println!("[{}] My own Priority {} - {}", req_id, own_priority, init_f);

        for server in &peer_servers {
            let msg = Msg {
                sender,
                receiver: server.1,
                msg_type: Type::ElectionRequest(*own_priority),
                payload: Some(req_id.clone()),
            };
            let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
            election_socket
                .send_to(&serialized_msg, server.1)
                .await
                .unwrap();
        }
        if !data.elections_initiated_by_me.contains(&req_id){
            data.elections_initiated_by_me.insert(req_id.clone());
        }
        drop(data);
        println!("[{}] Waiting for ok msg - {}", req_id, init_f);
        sleep(Duration::from_millis(1000)).await;

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

async fn handle_client(
    id: u32,
    msg: Msg,
    service_socket: Arc<UdpSocket>,
    election_socket: Arc<UdpSocket>,
    stats: Arc<Mutex<ServerStats>>,
    src_addr: SocketAddr,
) {
    let req_id = format!("{}:{}", msg.sender, id);
    println!("Handling Request {}", req_id);
    let mut data = stats.lock().await;
    data.requests_buffer
        .entry(req_id.clone())
        .or_insert_with(|| msg.clone());

    if data.running_elections.contains_key(&req_id) {
        println!("Election for this request is initialized, Buffering request!");
    } else {
        println!("[{}] Buffering", req_id);
        let random = {
            let mut rng = rand::thread_rng();
            rng.gen_range(10..30)
        } as u64;
        sleep(Duration::from_millis(random)).await;

        if !data.running_elections.contains_key(&req_id) {
            drop(data);
            send_election_msg(service_socket, election_socket, stats, req_id, true).await;
        }
    }
}

async fn handle_request(
    buffer: &[u8],
    src_addr: std::net::SocketAddr,
    service_socket: Arc<UdpSocket>,
    election_socket: Arc<UdpSocket>,
    stats: &Arc<Mutex<ServerStats>>,
) {
    let msg: Msg = match serde_cbor::de::from_slice(&buffer[..]) {
        Ok(msg) => msg,
        Err(_) => return,
    };
    println!("Received message :\n{:?}", msg);

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
            println!("handling election!");
            let req_id = msg.payload.clone().unwrap();
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
            println!("Handlign Broadcast!");
            let req_id = msg.payload.clone().unwrap();
            handle_coordinator(stats.to_owned(), req_id).await;
        }
        Type::OKMsg(priority) => {
            println!("Handling OK");
            let req_id = msg.payload.clone().unwrap();
            handle_ok_msg(req_id, stats.to_owned()).await;
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
    let service_socket = Arc::new(service_socket);
    let service_socket2 = Arc::clone(&service_socket);

    let election_socket = UdpSocket::bind(ip_elec).await.unwrap();
    println!("Server (election) listening on {ip_elec}");
    let election_socket = Arc::new(election_socket);
    let election_socket2 = Arc::clone(&election_socket);

    let stats = Arc::new(Mutex::new(stats));
    let stats_service = Arc::clone(&stats);
    let stats_election = Arc::clone(&stats);

    let mut service_buffer: [u8; 2048] = [0; 2048];
    let mut election_buffer: [u8; 2048] = [0; 2048];

    let h1 = tokio::spawn({
        async move {
            loop {
                match service_socket.recv_from(&mut service_buffer).await {
                    Ok((bytes_read, src_addr)) => {
                        let service_socket = Arc::clone(&service_socket);
                        let election_socket = Arc::clone(&election_socket2);
                        let stats_clone = Arc::clone(&stats_service);
                        tokio::spawn(async move {
                            handle_request(
                                &service_buffer[..bytes_read],
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

    let h2 = tokio::spawn({
        async move {
            loop {
                match election_socket.recv_from(&mut election_buffer).await {
                    Ok((bytes_read, src_addr)) => {
                        let service_socket = Arc::clone(&service_socket2);
                        let election_socket = Arc::clone(&election_socket);
                        let stats_clone = Arc::clone(&stats_election);
                        tokio::spawn(async move {
                            handle_request(
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
