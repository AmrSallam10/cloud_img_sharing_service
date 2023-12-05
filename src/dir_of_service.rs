use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use crate::commons::{self, Action};
use commons::{Msg, Type};
use tokio::net::UdpSocket;

#[derive(Debug)]
pub struct ClientDirOfService {
    entries: HashMap<SocketAddr, bool>,
}

impl ClientDirOfService {
    pub fn new() -> ClientDirOfService {
        ClientDirOfService {
            entries: HashMap::new(),
        }
    }
    // client requests the dir of service from a specific server (assumes election already done)
    pub async fn query(socket: Arc<UdpSocket>, server: SocketAddr) {
        let msg = Msg {
            sender: socket.local_addr().unwrap(),
            receiver: server,
            msg_type: Type::DirOfServQuery,
            payload: None,
        };
        // let serialized_msg = serde_json::to_string(&msg).unwrap();
        let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
        socket.send_to(&serialized_msg, server).await.unwrap();
    }

    // update own dir of service after recovery from failure
    pub async fn update(&mut self, d: HashMap<SocketAddr, bool>) {
        self.entries = d;
    }

    // subscribe
    pub async fn join(socket: Arc<UdpSocket>, servers: Vec<(SocketAddr, SocketAddr)>) {
        for server in servers {
            let msg = Msg {
                sender: socket.local_addr().unwrap(),
                receiver: server.0,
                msg_type: Type::DirOfServJoin,
                payload: None,
            };
            let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
            socket.send_to(&serialized_msg, server.0).await.unwrap();
        }
    }

    // unsubscribe
    pub async fn leave(socket: Arc<UdpSocket>, servers: Vec<(SocketAddr, SocketAddr)>) {
        for server in servers {
            let msg = Msg {
                sender: socket.local_addr().unwrap(),
                receiver: server.0,
                msg_type: Type::DirOfServLeave,
                payload: None,
            };
            let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
            socket.send_to(&serialized_msg, server.0).await.unwrap();
        }
    }
}

#[derive(Debug)]
pub struct ServerDirOfService {
    entries: HashMap<SocketAddr, bool>,
    pending_updates: HashMap<SocketAddr, HashMap<SocketAddr, Action>>,
}

impl ServerDirOfService {
    pub fn new() -> ServerDirOfService {
        ServerDirOfService {
            entries: HashMap::new(),
            pending_updates: HashMap::new(),
        }
    }

    // update own dir of service after recovery from failure
    pub async fn update(&mut self, d: HashMap<SocketAddr, bool>) {
        self.entries = d;
        println!("{:?}", self.entries);
    }

    pub async fn access_update(&mut self) {}

    // server wants updated Dir of service from peer servers
    pub async fn query(socket: Arc<UdpSocket>, peers: Vec<(SocketAddr, SocketAddr, SocketAddr)>) {
        for server in &peers {
            let msg = Msg {
                sender: socket.local_addr().unwrap(),
                receiver: server.0,
                msg_type: Type::DirOfServQuery,
                payload: None,
            };
            // let serialized_msg = serde_json::to_string(&msg).unwrap();
            let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
            socket.send_to(&serialized_msg, server.0).await.unwrap();
        }
    }

    // send dir of service back to the one sent a query
    pub async fn query_reply(&self, socket: Arc<UdpSocket>, src_addr: SocketAddr) {
        let sender = socket.local_addr().unwrap();
        let msg = Msg {
            sender,
            receiver: src_addr,
            msg_type: Type::DirOfServQueryReply(self.entries.clone()),
            payload: None,
        };
        let serialized_msg = serde_cbor::ser::to_vec(&msg).unwrap();
        socket.send_to(&serialized_msg, src_addr).await.unwrap();
    }

    // client wants to subscribe
    pub async fn client_join(&mut self, src_addr: SocketAddr) {
        let addr = SocketAddr::new(src_addr.ip(), src_addr.port() + 1);
        self.entries
            .entry(addr)
            .and_modify(|value| *value = true)
            .or_insert(true);
        println!("{:?}", self.entries);
    }

    // client wants to unsubscribe
    pub async fn client_leave(&mut self, src_addr: SocketAddr) {
        let addr = SocketAddr::new(src_addr.ip(), src_addr.port() + 1);
        self.entries
            .entry(addr)
            .and_modify(|value| *value = false)
            .or_insert(false);
        println!("{:?}", self.entries);
    }
}
