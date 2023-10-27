#![allow(dead_code, unused_variables, unused_imports)]
extern crate serde;
extern crate serde_json;
extern crate serde_derive;
use std::net::{UdpSocket, SocketAddr};
use std::{fs, env, thread};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Arc;
// use tokio::net::UdpSocket;
use tokio::time::{Duration, timeout};
// use serde_json::Result;
// use serde_derive::Serialize;


// struct to hold server states (modify as needed)
struct ServerStats {
    election: bool,
    elec_p: u32,
    coordinator: bool,
    up: bool,
}

#[derive(Serialize, Deserialize)]
enum Type {
    ClientRequest(u32),
    ElectionRequest(u32),
    OKMsg(u32),
    CoordinatorBrdCast(String),
}
#[derive(Serialize, Deserialize)]
struct MSG {
    sender: String,
    receiver: String, 
    msg_type: Type,
    payload: Option<String>,
}

// async fn receive_response(socket: &UdpSocket) -> Option<Vec<u8>> {
//     let mut buffer = vec![0; 1024];
//     match socket.recv_from(&mut buffer).await {
//         Ok((size, _)) => Some(buffer[..size].to_vec()),
//         Err(_) => None,
//     }
// }

fn is_reachable(ip: &str) -> bool {
    let output = Command::new("ping")
        .arg("-c")
        .arg("1") // Send 1 ping packet
        .arg(ip)
        .output()
        .expect("Failed to execute ping command");

    // Check the exit status
    output.status.success()
}

fn send_ok(src_addr: std::net::SocketAddr, socket: &UdpSocket) {
    // let mut rng = rand::thread_rng();
    // let random_number = rng.gen_range(1..10);
    // let msg = MSG {
    //     sender: socket.local_addr().unwrap().to_string(), 
    //     receiver: src_addr.to_string(), 
    //     msg_type: Type::OKMsg(random_number),
    //     payload: None,
    // };
    // let serialized_msg = serde_json::to_string(&msg).unwrap();
    // socket.send_to(serialized_msg.as_bytes(), src_addr.to_string().as_str())
    //                     .expect("error in sending election message");
}

fn initiate_election(servers: &Vec<String>, socket: &UdpSocket, stats: &mut ServerStats) {
    // let addr = socket.local_addr().unwrap().to_string();
    // let mut rng = rand::thread_rng();
    // let random_number = rng.gen_range(1..10);
    // stats.set_election();
    // stats.elec_p = random_number;
    // // let mut responses = Vec::new();

    // for server in servers {
    //     if is_reachable(server.as_str()){
    //         let msg = MSG {
    //             sender: addr.clone(),
    //             receiver: server.clone(), 
    //             msg_type: Type::ElectionRequest(random_number),
    //             payload: None,
    //         };

    //         let serialized_msg = serde_json::to_string(&msg).unwrap();
    //         socket.send_to(serialized_msg.as_bytes(), server.as_str())
    //                             .expect("error in sending election message");
            
    //         // listen_for_ok_responses(socket, &mut responses);
    //     } else {}
    // }
}

// async fn listen_for_ok_responses(socket: &UdpSocket, ok_responses: &mut Vec<MSG>) {
//     let mut buf = [0; 1024];
//     match socket.recv_from(&mut buf) {
//         Ok((bytes, _)) => {
//             let received_data = &buf[0..bytes];
//             if let Ok(response) = serde_json::from_slice::<MSG>(received_data) {
//                 if let Type::OKMsg(p) = response.msg_type {
//                     ok_responses.push(response);
//                 }
//             }
//         }
//         Err(err) => {
//             // Handle receive error
//         }
//     }
// }

fn handle_ok_msg(src_addr: std::net::SocketAddr, p: u32, socket: &UdpSocket, stats: &mut ServerStats) {

}

fn handle_election(src_addr: std::net::SocketAddr, socket: &UdpSocket, servers: &Vec<String>, stats: &mut ServerStats ) {

}

fn handle_coordinator(stats: &mut ServerStats, s: String) {

}

fn broadcast_coordinator() {

}

fn handle_client(src_addr: std::net::SocketAddr, servers: &Vec<String>, socket: &UdpSocket, msg: MSG, stats: &mut ServerStats){
    let addr = socket.local_addr().unwrap().to_string();
    println!("Received request from {}: {}", src_addr, msg.payload.unwrap());
    let response = "Hello! This is SERVER ".to_string() + &addr;
    socket.send_to(response.as_bytes(), src_addr).expect("Failed to send response");
}

fn handle_request(buffer: &[u8], src_addr: std::net::SocketAddr, servers: &Vec<String>, socket: &UdpSocket, stats: &mut ServerStats) {
    // Parse the message to know hpw to handle it
    let request: &str = std::str::from_utf8(buffer).expect("Failed to convert to UTF-8");
    let msg: MSG = serde_json::from_str(request).unwrap();

    // Handle request depending on the type
    // simple way, it need to be updated later
    // any exchanged messages must be uniquely identified by Ip and id of the client 
    // Initializer need to keep track of UP servers and the Ok msgs it received
    //
    match msg.msg_type {
        Type::ClientRequest(n)=> {
            // run election if not initiated (check stat) for the unique (IP + ID) of the request
            // if in election state and received same request (IP + ID), don't call for election
            // Update stats and Buffer client request
            // 
        },
        Type::ElectionRequest(p) => {
            // if I'm an election initializer of the same request, send ok msg only if sender is of higher priority
            // if not an initializer, send back an ok msg with your priority
            // 
        },
        Type::CoordinatorBrdCast(s) => {
            // If the encapsulated Ip is mine, I handle the client request
            // If not me, there will be another server handling this request and the election is over
            // update stats accordingly
            //
        },
        Type::OKMsg(p) => {
            // update stats (received ok msgs)
            // if i am an initializer and I have all Oks, I send coordinator msg and I handle the client request  
        }
    }
}

fn initialize(filepath: &str, ip: SocketAddr) -> Vec::<String> {
    let mut servers = Vec::<String>::new();
    let contents = fs::read_to_string(filepath).expect("Should have been able to read the file");
    for line in contents.lines(){
        if line != ip.to_string(){
            servers.push(line.to_string());
        }
    }
    servers
}

fn main() {
    let mut stats = ServerStats::new();
    let args: Vec<_> = env::args().collect();
    
    let ip: SocketAddr = args[1].as_str().parse().unwrap();
    let ip_elec = SocketAddr::new(ip.ip(), ip.port() + 1);

    let filepath = "./servers.txt";
    let servers = initialize(filepath, ip);  
    println!("{:?}", servers);  

    let service_socket = UdpSocket::bind(ip).expect("Failed to bind to address");
    println!("Server (service) listening on {ip}");

    let election_socket = UdpSocket::bind(ip_elec).expect("Failed to bind to address");
    println!("Server (election) listening on {ip_elec}");

    let mut buffer = [0; 1024];

    loop {
        match service_socket.recv_from(&mut buffer) {
            Ok((bytes_read, src_addr)) => {
                let socket_clone = service_socket.try_clone().expect("couldn't clone the socket");
                handle_request(&buffer[..bytes_read], src_addr, &servers, &socket_clone, &mut stats)
            }
            Err(e) => {
                eprintln!("Error receiving data: {}", e);
            }
        }
    }
 
}

impl ServerStats {
    fn new() -> ServerStats {
        ServerStats {
            election: false, 
            elec_p: 0,
            coordinator: false, 
            up: true, 
        }
    }

    fn set_election(&mut self){
        self.election = true;
    }
}