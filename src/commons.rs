use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};

use serde_derive::{Deserialize, Serialize};

pub const BUFFER_SIZE: usize = 32768;
pub const FRAG_SIZE: usize = 8000;
pub const BLOCK_SIZE: usize = 8;
pub const TIMEOUT_MILLIS: usize = 2000;
pub const SERVICE_PORT: usize = 8080;
pub const ELECTION_PORT: usize = 8081;
pub const SERVICE_SENDBACK_PORT: usize = 8082;
pub const SERVERS_FILEPATH: &str = "./servers.txt";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Fragment {
    pub msg_id: String,
    pub block_id: u32,
    pub frag_id: u32,
    pub msg_len: u32, // as bytes
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Type {
    ClientRequest(u32),
    ElectionRequest(f32),
    OKMsg(f32),
    CoordinatorBrdCast(String),
    Ack(String, u32),
    Fragment(Fragment),
    Fail(u32),
    DirOfServQuery,
    DirOfServQueryReply(HashMap<SocketAddr, bool>),
    DirOfServJoin,
    DirOfServLeave,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Msg {
    pub sender: SocketAddr,
    pub receiver: SocketAddr,
    pub msg_type: Type,
    pub payload: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BigMessage {
    pub data: Vec<u8>,
    pub msg_len: u32,
    pub received_len: u32,
    pub received_frags: HashSet<u32>,
}

impl BigMessage {
    pub fn default_msg() -> Self {
        Self {
            data: vec![0; 0],
            msg_len: 0,
            received_len: 0,
            received_frags: HashSet::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Image {
    pub dims: (u32, u32),
    pub data: Vec<u8>,
}
