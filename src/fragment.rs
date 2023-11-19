use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

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

pub async fn client_send(data: Vec<u8>, socket: Arc<UdpSocket>, address: &str, msg_id: &str) {
    let frag_num = (data.len() + FRAG_SIZE - 1) / FRAG_SIZE; // a shorthand for ceil()
    let block_num = (frag_num + BLOCK_SIZE - 1) / BLOCK_SIZE; // a shorthand for ceil()
    let msg_len = data.len();

    let mut buffer = [0; BUFFER_SIZE];
    println!("{}", msg_id);

    let mut curr_block = 0;
    while curr_block < block_num {
        let frag_st = BLOCK_SIZE * curr_block;
        let frag_end = min(BLOCK_SIZE * (curr_block + 1), frag_num);
        // construct and send fragments of the current block
        for frag_id in frag_st..frag_end {
            let st_idx = FRAG_SIZE * frag_id;
            let end_idx = min(FRAG_SIZE * (frag_id + 1), msg_len);

            let data_size = end_idx - st_idx;

            let mut frag_data = vec![0; data_size];

            frag_data.copy_from_slice(&data[st_idx..end_idx]);

            let frag = Fragment {
                msg_id: String::from(msg_id),
                block_id: curr_block as u32,
                frag_id: frag_id as u32,
                msg_len: msg_len as u32,
                data: frag_data,
            };

            let msg = Msg {
                sender: socket.local_addr().unwrap(),
                receiver: SocketAddr::from_str(address).unwrap(),
                msg_type: Type::Fragment(frag.clone()),
                payload: None,
            };

            println!(
                "Sending msg wrapping fragment {} of size {}.",
                frag_id, data_size
            );

            let msg = serde_cbor::ser::to_vec(&msg).unwrap();
            socket
                .send_to(&msg, address)
                .await
                .expect("Failed to send!");
        }

        let sleep = time::sleep(Duration::from_millis(TIMEOUT_MILLIS as u64));
        tokio::pin!(sleep);

        println!("Waiting for an ACK for block {}", curr_block);

        // wait for an ack
        tokio::select! {
            Ok((bytes_read, _)) = socket.recv_from(&mut buffer) => {
                // wait for the ack

                if let Ok(msg) = serde_cbor::de::from_slice::<Msg>(&buffer[..bytes_read]){
                    println!("{:?}", msg);

                    // TODO: Need logic to handle unexpected messages without breaking select!
                    //       Consider checking if conditions in the matching of branch conditions
                    if let Type::Ack(_msg_id, block_id) = msg.msg_type{
                            if block_id == curr_block as u32{
                                curr_block += 1;
                            }
                    };
                };
            }
            _ = &mut sleep => {
                println!("timeout");
            }
        }
    }
}

pub async fn server_send(
    data: Vec<u8>,
    socket: Arc<UdpSocket>,
    address: &str,
    msg_id: &str,
    mut rx: mpsc::Receiver<u32>,
) {
    let frag_num = (data.len() + FRAG_SIZE - 1) / FRAG_SIZE; // a shorthand for ceil()
    let block_num = (frag_num + BLOCK_SIZE - 1) / BLOCK_SIZE; // a shorthand for ceil()
    let msg_len = data.len();

    let mut curr_block = 0;
    while curr_block < block_num {
        let frag_st = BLOCK_SIZE * curr_block;
        let frag_end = min(BLOCK_SIZE * (curr_block + 1), frag_num);
        // construct and send fragments of the current block
        for frag_id in frag_st..frag_end {
            let st_idx = FRAG_SIZE * frag_id;
            let end_idx = min(FRAG_SIZE * (frag_id + 1), msg_len);

            let data_size = end_idx - st_idx;

            let mut frag_data = vec![0; data_size];

            frag_data.copy_from_slice(&data[st_idx..end_idx]);

            let frag = Fragment {
                msg_id: String::from(msg_id),
                block_id: curr_block as u32,
                frag_id: frag_id as u32,
                msg_len: msg_len as u32,
                data: frag_data,
            };

            let msg = Msg {
                sender: socket.local_addr().unwrap(),
                receiver: SocketAddr::from_str(address).unwrap(),
                msg_type: Type::Fragment(frag.clone()),
                payload: None,
            };

            println!(
                "[{}] Sending message wrapping fragment {} of size {}.",
                msg_id, frag_id, data_size
            );

            let msg = serde_cbor::ser::to_vec(&msg).unwrap();
            socket
                .send_to(&msg, address)
                .await
                .expect("Failed to send!");
        }

        let sleep = time::sleep(Duration::from_millis(TIMEOUT_MILLIS as u64));
        tokio::pin!(sleep);

        println!("Waiting for an ACK for block {}", curr_block);

        // wait for an ack
        tokio::select! {
            Some(block_id) = rx.recv() => {
                // wait for the ack


                // TODO: Need logic to handle unexpected messages without breaking select!
                //       Consider checking if conditions in the matching of branch conditions
            if block_id == curr_block as u32{
                curr_block += 1;
            }
        }
            _ = &mut sleep => {
                println!("timeout");
            }
        }
    }
}

pub async fn recieve(socket: Arc<UdpSocket>) -> Vec<u8> {
    let mut buffer = [0; BUFFER_SIZE];

    let mut map: HashMap<String, BigMessage> = HashMap::new();

    loop {
        match socket.recv_from(&mut buffer).await {
            Ok((bytes_read, src_addr)) => {
                println!("{} bytes from {}.", bytes_read, src_addr);

                if let Ok(msg) = serde_cbor::de::from_slice::<Msg>(&buffer[..bytes_read]) {
                    let frag = match msg.msg_type {
                        Type::Fragment(frag) => frag,
                        _ => {
                            println!("Could not parse fragment");
                            continue;
                        }
                    };
                    println!("[{}] Received fragment {}.", frag.msg_id, frag.frag_id);
                    let mut new_frag = false;

                    let st_idx = FRAG_SIZE * (frag.frag_id as usize);
                    let end_idx = min(
                        FRAG_SIZE * (frag.frag_id + 1) as usize,
                        frag.msg_len as usize,
                    );

                    // if this is the first fragment create a new entry in the map
                    if let std::collections::hash_map::Entry::Vacant(e) =
                        map.entry(frag.msg_id.clone())
                    {
                        let mut msg_data = vec![0; frag.msg_len as usize];

                        msg_data[st_idx..end_idx].copy_from_slice(&frag.data);

                        let mut received_frags = HashSet::new();
                        received_frags.insert(frag.frag_id);

                        let msg = BigMessage {
                            data: msg_data,
                            msg_len: frag.msg_len,
                            received_len: (end_idx - st_idx) as u32,
                            received_frags,
                        };

                        if msg.received_len == msg.msg_len
                            || (msg.received_len as usize / FRAG_SIZE) % BLOCK_SIZE == 0
                        {
                            // send ack
                            let block_id = (msg.received_len as usize + FRAG_SIZE * BLOCK_SIZE - 1)
                                / (FRAG_SIZE * BLOCK_SIZE)
                                - 1;
                            println!("Sending ACK for block {}", block_id);
                            let receiver: SocketAddr =
                                format!("{}:{}", src_addr.ip(), src_addr.port() - 2)
                                    .parse()
                                    .unwrap();
                            let ack = Msg {
                                msg_type: Type::Ack(frag.msg_id, block_id as u32),
                                sender: socket.local_addr().unwrap(),
                                receiver,
                                payload: None,
                            };

                            println!("{:?}", ack);
                            let ack = serde_cbor::ser::to_vec(&ack).unwrap();
                            socket
                                .send_to(&ack, receiver.to_string())
                                .await
                                .expect("Failed to send!");
                        }
                        if msg.received_len == msg.msg_len {
                            println!("Full message is received!");
                            let msg = msg.to_owned();
                            e.insert(msg.clone());
                            return msg.data;
                        }
                        e.insert(msg);
                    } else {
                        // should not be needed since we know key exists
                        let _default_msg = BigMessage::default_msg();
                        let big_msg = map.entry(frag.msg_id.clone()).or_insert(_default_msg);

                        if !(big_msg.received_frags.contains(&frag.frag_id)) {
                            big_msg.data[st_idx..end_idx].copy_from_slice(&frag.data);

                            big_msg.received_len += (end_idx - st_idx) as u32;
                            big_msg.received_frags.insert(frag.frag_id);
                            new_frag = true;
                        }

                        if new_frag
                            && (big_msg.received_len == big_msg.msg_len
                                || (big_msg.received_len as usize / FRAG_SIZE) % BLOCK_SIZE == 0)
                        {
                            // send ack
                            let block_id = (big_msg.received_len as usize + FRAG_SIZE * BLOCK_SIZE
                                - 1)
                                / (FRAG_SIZE * BLOCK_SIZE)
                                - 1;
                            println!("Sending ACK for block {}", block_id);
                            let receiver: SocketAddr =
                                format!("{}:{}", src_addr.ip(), src_addr.port() - 2)
                                    .parse()
                                    .unwrap();
                            let ack = Msg {
                                msg_type: Type::Ack(frag.msg_id, block_id as u32),
                                sender: socket.local_addr().unwrap(),
                                receiver,
                                payload: None,
                            };

                            println!("{:?}", ack);
                            let ack = serde_cbor::ser::to_vec(&ack).unwrap();
                            socket
                                .send_to(&ack, receiver.to_string())
                                .await
                                .expect("Failed to send!");
                        }
                        if new_frag && big_msg.received_len == big_msg.msg_len {
                            println!("Full message is received!");
                            let big_msg = big_msg.to_owned();
                            return big_msg.data;
                        }
                    }
                } else {
                    continue;
                };
            }
            Err(e) => {
                eprintln!("Error receiving data: {}", e);
            }
        }
    }
}
