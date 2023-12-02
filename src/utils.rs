use crate::commons::{ELECTION_PORT, SERVICE_PORT, SERVICE_SENDBACK_PORT};
use std::{fs, net::SocketAddr};

pub async fn get_peer_servers(
    filepath: &str,
    own_ips: (SocketAddr, SocketAddr, SocketAddr),
    mode: &str,
) -> Vec<(SocketAddr, SocketAddr, SocketAddr)> {
    let mut servers = Vec::<(SocketAddr, SocketAddr, SocketAddr)>::new();
    let contents = fs::read_to_string(filepath).expect("Should have been able to read the file");
    for ip in contents.lines() {
        let (ip_service, ip_elec, ip_send) = get_ips(ip, mode).await;
        if own_ips.0 != ip_service {
            servers.push((ip_service, ip_elec, ip_send));
        }
    }
    servers
}

pub async fn get_ips(ip: &str, mode: &str) -> (SocketAddr, SocketAddr, SocketAddr) {
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

pub fn get_req_id_log(filepath: &str) -> u32 {
    match fs::read_to_string(filepath) {
        Ok(contents) => contents.parse::<u32>().unwrap_or(1),
        Err(_) => 0, // Default value in case of an error or missing file
    }
}

pub fn get_cloud_servers(filepath: &str, mode: &str) -> Vec<(SocketAddr, SocketAddr)> {
    let contents = fs::read_to_string(filepath).expect("Should have been able to read the file");
    if mode == "local" {
        let servers: Vec<(SocketAddr, SocketAddr)> = contents
            .lines()
            .map(|addr| {
                let elec_ip: SocketAddr = addr.parse().unwrap();
                let serv_ip: SocketAddr = format!("{}:{}", elec_ip.ip(), elec_ip.port() - 1)
                    .parse()
                    .unwrap();
                (serv_ip, elec_ip)
            })
            .collect();
        servers
    } else {
        let servers: Vec<(SocketAddr, SocketAddr)> = contents
            .lines()
            .map(|addr| {
                let elec_ip: SocketAddr = format!("{}:{}", addr, ELECTION_PORT).parse().unwrap();
                let serv_ip: SocketAddr = format!("{}:{}", addr, SERVICE_PORT).parse().unwrap();

                (serv_ip, elec_ip)
            })
            .collect();
        servers
    }
}

pub fn create_output_dirs() {
    let base_directory = "output";
    let encoded_directory = "output/encoded";
    let decoded_directory = "output/decoded";

    // Attempt to create the entire directory structure
    if let Err(err) = fs::create_dir_all(encoded_directory) {
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

    if let Err(err) = fs::create_dir_all(decoded_directory) {
        if err.kind() == std::io::ErrorKind::AlreadyExists {
            println!("Directory '{}' already exists.", decoded_directory);
        } else {
            println!(
                "Error creating directory '{}': {:?}",
                decoded_directory, err
            );
        }
    }
}

pub fn get_pic_paths(filepath: &str) -> Vec<String> {
    let contents = fs::read_to_string(filepath).expect("Should have been able to read the file");
    let pic_paths: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    pic_paths
}
