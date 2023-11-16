#![allow(dead_code, unused_variables, unused_imports)]
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, UdpSocket};
use std::process::Command;
use std::str::FromStr;
use std::{
    env, fs, thread,
    time::{Duration, Instant},
};

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
    let contents = fs::read_to_string(filepath).expect("Should have been able to read the file");
    let servers: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    servers
}

fn get_req_id_log(filepath: &str) -> u32 {
    match fs::read_to_string(filepath) {
        Ok(contents) => contents.parse::<u32>().unwrap_or(1),
        Err(_) => 0, // Default value in case of an error or missing file
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<_> = env::args().collect();
    let ip: &str = args[1].as_str();
    let ip = SocketAddr::from_str(ip).unwrap();
    let servers_filepath = "./servers.txt";
    let req_id_log_filepath = "./req_id_log.txt";
    let servers = get_servers(servers_filepath);
    let socket = UdpSocket::bind(ip)?;
    let mut time: u128 = 0;
    let iterations = 200;
    for i in 0..iterations {
        // let mut req_id_log = get_req_id_log(req_id_log_filepath);
        let start = Instant::now();
        for server in &servers {
            let server = SocketAddr::from_str(server.as_str()).unwrap();
            let msg = Msg {
                sender: socket.local_addr().unwrap(),
                receiver: server,
                msg_type: Type::ClientRequest(i + 1),
                payload: Some(i.to_string()),
            };
            let serialized_msg = serde_json::to_string(&msg).unwrap();
            socket.send_to(serialized_msg.as_bytes(), server)?;
        }
        let mut buffer = [0; 1024];
        let (bytes_read, _source) = socket.recv_from(&mut buffer)?;
        let duration = start.elapsed().as_micros();
        time += duration;
        let response: u32 = std::str::from_utf8(&buffer[..bytes_read])
            .unwrap()
            .parse()
            .unwrap();
        // println!("received req {} in duration {}", i, duration);
        println!("[{}] : {}", _source, response);
        if response != i + 1 {
            println!("Error!")
        };
        // req_id_log += 1;
        // fs::write(req_id_log_filepath, req_id_log.to_string())?;
    }
    println!("Avg response time = {}", time as f64 / iterations as f64);
    Ok(())
}
