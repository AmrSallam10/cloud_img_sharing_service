use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::{self, Duration};

const BUFFER_SIZE: usize = 32768;
const FRAG_SIZE: usize = 4096;
const BLOCK_SIZE: usize = 2;
const TIMEOUT_MILLIS: usize = 2000;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Fragment {
    pub msg_id: String,
    pub frag_id: u32,
    pub msg_len: u32, // as bytes
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
enum Type {
    Ack(u32),
}
#[derive(Serialize, Deserialize, Debug)]
struct Msg {
    msg_type: Type,
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

pub async fn send(data: Vec<u8>, socket: Arc<UdpSocket>, address: &str, msg_id: &str) {
    let frag_num = (data.len() + FRAG_SIZE - 1) / FRAG_SIZE; // a shorthand for ceil()
    let block_num = (frag_num + BLOCK_SIZE - 1) / BLOCK_SIZE; // a shorthand for ceil()
    let msg_len = data.len();
    // let msg_id = "1";

    let mut buffer = [0; BUFFER_SIZE];

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
                frag_id: frag_id as u32,
                msg_len: msg_len as u32,
                data: frag_data,
            };

            println!("Sending fragment {} of size {}.", frag_id, data_size);

            let frag = serde_cbor::ser::to_vec(&frag).unwrap();
            socket
                .send_to(&frag, address)
                .await
                .expect("Failed to send!");
        }

        let sleep = time::sleep(Duration::from_millis(TIMEOUT_MILLIS as u64));
        tokio::pin!(sleep);

        println!(
            "Waiting for an ACK for block {}. Timeout {} milli seconds.",
            curr_block, TIMEOUT_MILLIS
        );

        // wait for an ack
        tokio::select! {
            Ok((bytes_read, _)) = socket.recv_from(&mut buffer) => {
                // wait for the ack

                let msg: Msg = serde_cbor::de::from_slice(&buffer[..bytes_read]).unwrap();
                println!("{:?}", msg);

                // TODO: Need logic to handle unexpected messages without breaking select!
                //       Consider checking if conditions in the matching of branch conditions
                curr_block += 1;
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

                let frag: Fragment = serde_cbor::de::from_slice(&buffer[..bytes_read]).unwrap();
                println!("Received fragment {}.", frag.frag_id);

                let st_idx = FRAG_SIZE * (frag.frag_id as usize);
                let end_idx = min(
                    FRAG_SIZE * (frag.frag_id + 1) as usize,
                    frag.msg_len as usize,
                );

                // if this is the first fragment create a new entry in the map
                if let std::collections::hash_map::Entry::Vacant(e) = map.entry(frag.msg_id.clone())
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

                    e.insert(msg);
                } else {
                    // should not be needed since we know key exists
                    let _default_msg = BigMessage::default_msg();
                    let big_msg = map.entry(frag.msg_id).or_insert(_default_msg);

                    if !(big_msg.received_frags.contains(&frag.frag_id)) {
                        big_msg.data[st_idx..end_idx].copy_from_slice(&frag.data);

                        big_msg.received_len += (end_idx - st_idx) as u32;
                        big_msg.received_frags.insert(frag.frag_id);
                    }

                    if big_msg.received_len == big_msg.msg_len
                        || (big_msg.received_len as usize / FRAG_SIZE) % BLOCK_SIZE == 0
                    {
                        // send ack
                        let block_id = (big_msg.received_len as usize + FRAG_SIZE * BLOCK_SIZE - 1)
                            / (FRAG_SIZE * BLOCK_SIZE)
                            - 1;
                        let ack = Msg {
                            msg_type: Type::Ack(block_id as u32),
                        };

                        let ack = serde_cbor::ser::to_vec(&ack).unwrap();
                        socket
                            .send_to(&ack, src_addr.to_string())
                            .await
                            .expect("Failed to send!");
                    }
                    if big_msg.received_len == big_msg.msg_len {
                        println!("Full message is received!");
                        let big_msg = big_msg.to_owned();
                        return big_msg.data;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error receiving data: {}", e);
            }
        }
    }
}
