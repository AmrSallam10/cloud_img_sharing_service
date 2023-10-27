#![allow(dead_code, unused_variables, unused_imports)]
extern crate serde;
extern crate serde_json;
extern crate serde_derive;
use std::net::UdpSocket; 
use std::{fs, env, thread, time::Duration};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Serialize, Deserialize)]
enum Type {
    ClientRequest(u32),
    ElectionRequest(u32),
    CoordinatorBrdCast,
}
#[derive(Serialize, Deserialize)]
struct MSG {
    sender: String,
    receiver: String, 
    msg_type: Type,
    payload: Option<String>,
}

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

fn get_servers(filepath: &str) -> Vec::<String> {
    let contents = fs::read_to_string(filepath).expect("Should have been able to read the file");
    let servers: Vec<String> =  contents.lines().map(|s| s.to_string()).collect();
    servers
}

fn main() -> std::io::Result<()> {
    let args: Vec<_> = env::args().collect();
    let ip: &str = args[1].as_str();
    let filepath = "./servers.txt";
    let servers = get_servers(filepath);
    // let msg = String::from("Hello! This is CLIENT1");
    
    let socket = UdpSocket::bind(ip)?;
    for server in servers {
        
        let msg = MSG {
            sender: ip.to_string(),
            receiver: server.clone(), 
            msg_type: Type::ClientRequest(1),
            payload: Some("Hello! This is CLIENT1".to_string()),
        };
        let serialized_msg = serde_json::to_string(&msg).unwrap();
        socket.send_to(serialized_msg.as_bytes(), server)?;
    }
    let mut buffer = [0; 1024];
    let (bytes_read, _source) = socket.recv_from(&mut buffer)?;
    let response = std::str::from_utf8(&buffer[..bytes_read]).unwrap();
    println!("Server response: {}", response);

    Ok(())
}