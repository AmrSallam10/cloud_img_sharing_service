#![allow(dead_code, unused_variables, unused_imports)]
extern crate serde;
extern crate serde_json;
extern crate serde_derive;
use std::net::SocketAddr;
use std::{fs, env};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::cmp::max;
use std::sync::Arc;
use tokio::time::{Duration, timeout, sleep};
use tokio::{sync::Mutex, net::UdpSocket};

use std::collections::HashSet;
use std::collections::HashMap;


#[derive(Clone)]
struct ServerDirectory {
    online_clients: HashSet<SocketAddr>,
    clients_per_server: HashMap<SocketAddr, HashSet<SocketAddr>>,
}
// struct to hold server states (modify as needed)
#[derive(Clone)]
struct ServerStats {
    elections_initiated_by_me: HashMap<String, (u32,u32)>, // req_id -> (own_p, #anticipated Oks)
    elections_received_oks: HashMap<String, u32>, // req_id -> #currently received Oks
    elections_leader: HashMap<String, (String,u32)>, // req_id -> (addr of currently chosen leader, its priority)
    running_elections: HashSet<String>,
    requests_buffer: HashMap<String, MSG>,
    peer_servers: Vec<(SocketAddr, SocketAddr)>,
    sockets_ips: (String, String),
    directory: ServerDirectory,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum Type {
    ClientRequest(u32),
    ElectionRequest(u32),
    OKMsg(u32),
    CoordinatorBrdCast(String),
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct MSG {
    sender: SocketAddr,
    receiver: SocketAddr, 
    msg_type: Type,
    payload: Option<String>,
}

fn is_reachable(ip: SocketAddr) -> bool {
    let output = Command::new("nc")
        .arg("-v")
        .arg("-u")
        .arg("-z")
        .arg("-w")
        .arg("1") 
        .arg(ip.ip().to_string())
        .arg(ip.port().to_string())
        .output()
        .expect("Failed to execute ping command");

    // Check the exit status
    output.status.success()
}


async fn initiate_election(socket: Arc<&UdpSocket>, stats: Arc<Mutex<ServerStats>>, req_id: String) {
    let stats_clone = stats.clone();
    let mut data  = stats_clone.lock().await;
    // abort election if someone already started it
    if data.running_elections.contains(&req_id) {
        println!("Aborting Election initialization!");
        return
    }
    
    let sender = socket.local_addr().unwrap();
    println!("{sender} Initiating Election!");
    
    let peer_servers: Vec<(SocketAddr, SocketAddr)> = data.get_peer_servers();
    println!("Inserting Request {req_id} in running elections");
    data.running_elections.insert(req_id.clone());
    println!("Inserting Request {req_id} in initiated elections");
    let own_p  = { 
        let mut rng = rand::thread_rng();
        rng.gen_range(1..1000) 
    } as u32;
    println!("My own Priority {}", own_p);

    let mut active_participatns = 0;

    for server in &peer_servers {
        if is_reachable(server.1){
            let msg = MSG {
                sender: sender.clone(),
                receiver: server.1.clone(), 
                msg_type: Type::ElectionRequest(own_p),
                payload: Some(req_id.clone()),
            };
            let serialized_msg = serde_json::to_string(&msg).unwrap();
            // println!("{}", serialized_msg);
            socket.send_to(serialized_msg.as_bytes(), server.1.clone()).await.unwrap();  

            active_participatns += 1;          
        } else {
        }
    }
    data.elections_initiated_by_me.insert(req_id.clone(), (own_p,active_participatns));
}

async fn handle_ok_msg(req_id: String, src_addr: SocketAddr, p: u32, socket: Arc<&UdpSocket>, 
    stats: Arc<Mutex<ServerStats>>) {
        let mut call = false;
        {
            let mut data = stats.lock().await; 
            if data.elections_initiated_by_me.contains_key(&req_id) {
                // this assume no server replies twice to an election request message
                {
                    let reply_count = data.elections_received_oks.entry(req_id.clone()).or_insert(0);
                    *reply_count += 1;
                }
                let own_addr  = socket.local_addr().unwrap().clone().to_string();
                let own_p = data.elections_initiated_by_me.get(&req_id.clone()).unwrap().clone().0;
                {
                    let curr_leader = data.elections_leader.entry(req_id.clone()).or_insert((own_addr, own_p));
                    if curr_leader.1 < p {
                        *curr_leader = (src_addr.to_string(),p); 
                    }
                }
                let reply_count = *data.elections_received_oks.get(&req_id).unwrap();
                // might be wrong logic, if the message is not there then this indicates wrong flow
                if reply_count == data.elections_initiated_by_me.get(&req_id.clone()).unwrap().1 {
                    let curr_leader = data.elections_leader.get(&req_id).unwrap().0.clone();
                    let peer_servers = data.peer_servers.clone();
                    let socket_clone = socket.clone();
                    broadcast_coordinator(socket_clone, curr_leader.clone(), peer_servers, req_id.clone()).await;
                    println!("{}, {}", curr_leader, socket.local_addr().unwrap().to_string());
                    if curr_leader == data.sockets_ips.0 || curr_leader == data.sockets_ips.1 {
                        call = true;
                    }
                }
            } else {
                // it is assumed that oks only are received by initializers
            }
        }
        if call {
            let stats_clone: Arc<Mutex<ServerStats>> = stats.clone();
            reply_to_client(socket, req_id, stats_clone).await;
        } else {}
}

async fn reply_to_client( socket: Arc<&UdpSocket>, req_id: String, stats: Arc<Mutex<ServerStats>>){
    let data = stats.lock().await;
    println!("Replying to Client");
    let addr = &socket.local_addr().unwrap().to_string();
    let response = format!("Hello! This is SERVER {addr} for req_id {req_id}\n"); 
    let target_addr = data.requests_buffer.get(&req_id).unwrap().sender;
    socket.send_to(response.as_bytes(), target_addr).await.unwrap();
}

async fn handle_election(p: u32, req_id: String, src_addr: std::net::SocketAddr, socket: Arc<&UdpSocket>, 
    stats: Arc<Mutex<ServerStats>> ) {
    // if I'm an election initializer of the same request, send ok msg only if sender is of higher priority
    // if not an initializer, send back an ok msg with your priority
    let mut stats = stats.lock().await;

    if stats.elections_initiated_by_me.contains_key(&req_id) {
        if p > stats.elections_initiated_by_me[&req_id].0 {
            let own_p = stats.elections_initiated_by_me[&req_id].0;
            let msg = MSG {
                sender: socket.local_addr().unwrap().clone(),
                receiver: src_addr, 
                msg_type: Type::OKMsg(own_p),
                payload: Some(req_id.clone()),
            };
            let serialized_msg = serde_json::to_string(&msg).unwrap();
            socket.send_to(serialized_msg.as_bytes(), src_addr).await.unwrap();
        } else {}
    } else {
        let own_p = socket.local_addr().unwrap().port() as u32;
        let msg = MSG {
            sender: socket.local_addr().unwrap().clone(),
            receiver: src_addr, 
            msg_type: Type::OKMsg(own_p),
            payload: Some(req_id.clone()),
        };
        let serialized_msg = serde_json::to_string(&msg).unwrap();
        socket.send_to(serialized_msg.as_bytes(), src_addr).await.unwrap();
    }

    if !stats.running_elections.contains(&req_id) {
        stats.running_elections.insert(req_id);
    } else {}
}


async fn broadcast_coordinator(socket: Arc<&UdpSocket>, leader: String, peer_servers: Vec<(SocketAddr, SocketAddr)>, 
    req_id: String) {
    // broadcast message
    for server in &peer_servers {
        let msg = MSG {
            sender: socket.local_addr().unwrap().clone(),
            receiver: server.1.clone(), 
            msg_type: Type::CoordinatorBrdCast(leader.clone()),
            payload: Some(req_id.clone()),
        };
        let serialized_msg = serde_json::to_string(&msg).unwrap();
        socket.send_to(serialized_msg.as_bytes(), server.1).await.unwrap();
    }
}
async fn handle_coordinator(stats: Arc<Mutex<ServerStats>>, coordinator_ip: String, socket: Arc<&UdpSocket>,
     req_id: String) {
        let stats_clone = stats.clone();
        let mut ips = (String::new(), String::new());
        {
            let data = stats.lock().await;
            ips = data.sockets_ips.clone();
        }
        if coordinator_ip ==  ips.0 || coordinator_ip ==  ips.1 {
            reply_to_client(socket, req_id, stats_clone).await;
        } else {
            let mut data = stats.lock().await;
            data.running_elections.remove(&req_id);
        }

}

async fn handle_client(n: u32, msg: &MSG, 
    socket: Arc<&UdpSocket>, stats: Arc<Mutex<ServerStats>>, src_addr: SocketAddr){
        
    let socket_clone = socket.clone();
    let mut req_id = msg.sender.clone().to_string();
    req_id.push_str(":"); // to avoid having port 32323 req 2 mixed with port 3232 req 32 
    req_id.push_str(&n.to_string());
    let req_id_clone = req_id.clone();
    println!("Handling Request {req_id}");
    let mut initiage_elec_flag = false;
    
    let stats_clone = stats.clone();
    {
        let mut data = stats.lock().await;
        if data.running_elections.contains(&req_id) {
            println!("Election for this request is initialized, Buffering request!");
            if !data.requests_buffer.contains_key(&req_id) {
                data.requests_buffer.insert(req_id, msg.clone());
            }
        } else {
            println!("Buffering {req_id}");
            if !data.requests_buffer.contains_key(&req_id) {
                data.requests_buffer.insert(req_id, msg.clone());
            }
            initiage_elec_flag = true;   
        }
    }

    // to reduce the chance of multiple initializers
    // println!("{:?}", socket.local_addr().unwrap().to_string());
    let random  = { 
        let mut rng = rand::thread_rng();
        rng.gen_range(10..30) 
    } as u64;
    
    sleep(Duration::from_millis(random)).await;
    if initiage_elec_flag { 
        initiate_election(socket, stats_clone, req_id_clone).await 
    } else {}
}

async fn handle_request(buffer: &[u8], src_addr: std::net::SocketAddr, 
    socket: Arc<&UdpSocket>, stats: &Arc<Mutex<ServerStats>>) {
        
    // Parse the message to know how to handle it
    let request: &str = std::str::from_utf8(buffer).expect("Failed to convert to UTF-8");
    let msg: MSG = match serde_json::from_str(request) {
        Ok(msg) => msg,
        Err(_) => {return}
    };
    println!(" Received message :\n{}", request);

    // Handle request depending on the type
    // simple way, it need to be updated later
    // any exchanged messages must be uniquely identified by Ip and id of the client 
    // Initializer need to keep track of UP servers and the Ok msgs it received
    //
    match msg.msg_type {
        Type::ClientRequest(n)=> {
            {
                let mut data = stats.lock().await;
                if !data.directory.client_exists(&src_addr) {
                    data.directory.register_client(src_addr);
                }
            }
            // run election if not initiated (check stat) for the unique (IP + ID) of the request
            // if in election state and received same request (IP + ID), don't call for election
            // Update stats and Buffer client request
            // 
            let stats_clone = stats.clone();
            handle_client(n, &msg, socket, stats_clone, src_addr).await;
        },
        Type::ElectionRequest(p) => {
            println!("handling election!");
            let stats_clone = stats.clone();
            let req_id = msg.payload.clone().unwrap();
            handle_election(p, req_id, src_addr, socket, stats_clone).await;
            // if I'm an election initializer of the same request, send ok msg only if sender is of higher priority
            // if not an initializer, send back an ok msg with your priority
            //
        },
        Type::CoordinatorBrdCast(coordinator_ip) => {
            println!("Handlign Broadcast!");
            let stats_clone = stats.clone();
            let req_id = msg.payload.clone().unwrap();
            handle_coordinator(stats_clone, coordinator_ip, socket, req_id).await;
            // If the encapsulated Ip is mine, I handle the client request
            // If not me, there will be another server handling this request and the election is over
            // update stats accordingly
            //
        },
        Type::OKMsg(p) => {
            println!("Handling OK");
            let stats_clone = stats.clone();
            let req_id = msg.payload.clone().unwrap();
            handle_ok_msg(req_id, src_addr, p, socket, stats_clone).await;
            // update stats (received ok msgs)
            // if i am an initializer and I have all Oks, I send coordinator msg and I handle the client request
            {
                let mut data = stats.lock().await;
                data.directory.display_online_clients();
            }

        }
    }
}

fn get_peer_servers(filepath: &str, own_ip: SocketAddr) -> Vec::<(SocketAddr, SocketAddr)> {
    let mut servers = Vec::<(SocketAddr, SocketAddr)>::new();
    let contents = fs::read_to_string(filepath).expect("Should have been able to read the file");
    for ip in contents.lines(){
        if ip != own_ip.to_string(){
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
    
    let ip: SocketAddr = args[1].as_str().parse().unwrap();
    let ip_elec = SocketAddr::new(ip.ip(), ip.port() + 1);

    let filepath: &str = "./servers.txt";
    stats.peer_servers = get_peer_servers(filepath, ip); 
    stats.sockets_ips = (ip.to_string(), ip_elec.to_string());

    let service_socket = UdpSocket::bind(ip).await.unwrap();
    println!("Server (service) listening on {ip}");

    let election_socket = UdpSocket::bind(ip_elec).await.unwrap();
    println!("Server (election) listening on {ip_elec}");

    let mut service_buffer = [0; 2048];
    let mut election_buffer = [0; 2048];
    let stats = Arc::new(Mutex::new(stats));
    let stats1 = stats.clone();
    let stats2 = stats.clone();

    let h1 = tokio::spawn(async move {
        loop{
            match service_socket.recv_from(&mut service_buffer).await {
                
                Ok((bytes_read, src_addr)) => {
                    let s = Arc::new(&service_socket);
                    let r = s.clone();
                    handle_request(&service_buffer[..bytes_read], src_addr, r,  &stats1).await;
                }
                Err(e) => {
                    eprintln!("Error receiving data: {}", e);
                }
            }
        }
    });
    
    let h2 = tokio::spawn(async move {
        loop{
            match election_socket.recv_from(&mut election_buffer).await {
                
                Ok((bytes_read, src_addr)) => {
                    let s = Arc::new(&election_socket);
                    let r = s.clone();
                    handle_request(&election_buffer[..bytes_read], src_addr, r, &stats2).await;
                }
                Err(e) => {
                    eprintln!("Error receiving data: {}", e);
                }
            }
        }
    });

    h1.await.unwrap();
    h2.await.unwrap();
}

impl ServerDirectory {
    fn new() -> Self {
        ServerDirectory {
            online_clients: HashSet::new(),
            clients_per_server: HashMap::new(),
        }
    }
    fn register_client(&mut self, client_addr: SocketAddr) {
        self.online_clients.insert(client_addr);

    }

    fn unregister_client(&mut self, client_addr: &SocketAddr) {
        self.online_clients.remove(client_addr);
    }

    fn get_online_clients(&self) -> HashSet<SocketAddr> {
        self.online_clients.clone()
    }


    fn get_clients_for_server(&self, server_addr: &SocketAddr) -> Option<HashSet<SocketAddr>> {
        self.clients_per_server.get(server_addr).cloned()
    }

    fn client_exists(&self, client_addr: &SocketAddr) -> bool {
        self.online_clients.contains(client_addr)
    }

    fn display_online_clients(&self) {
        println!("Online Clients:");
        for client_addr in &self.online_clients {
            println!("{}", client_addr);
        }
    }
}
impl ServerStats {
    fn new() -> ServerStats {
        ServerStats {
            requests_buffer: HashMap::new(),
            elections_initiated_by_me: HashMap::new(),
            elections_leader: HashMap::new(),
            elections_received_oks: HashMap::new(),
            running_elections: HashSet::new(), 
            peer_servers: Vec::new(),
            sockets_ips: (String::new(), String::new()),
            directory: ServerDirectory::new()
        }
    }

    fn get_peer_servers(&self) -> Vec<(SocketAddr, SocketAddr)>{
        self.peer_servers.clone()
    }
}