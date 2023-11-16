use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use std::collections::HashMap;
use std::collections::HashSet;
use std::cmp::min;
use tokio::fs;


#[derive(Serialize, Deserialize, Clone, Debug)]
struct Fragment {
    msg_id: String,
    frag_id: u32,
    msg_len: u32,
    data: Vec<u8>
}

#[derive(Debug)]
struct BigMessage {
    data: Vec<u8>,
    msg_len: u32,
    received_len: u32,
    received_frags: HashSet<u32>
}

impl BigMessage {
    fn default_msg() -> Self {
        Self {
            data: vec![0;0],
            msg_len: 0,
            received_len: 0,
            received_frags: HashSet::new()
        }
    }
}


#[derive(Serialize, Deserialize, Debug)]
enum Type {
    ACK(u32)
}
#[derive(Serialize, Deserialize, Debug)]
struct MSG {
    msg_type: Type,
}


#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("0.0.0.0:9999").await.expect("Failed to bind to 0.0.0.0:9999");
    
    const BUFFER_SIZE: usize = 32768;
    const FRAG_SIZE: usize = 4096;
    const BLOCK_SIZE: usize = 2;

    let mut buffer = [0; BUFFER_SIZE];
    
    let mut map: HashMap<String, BigMessage> = HashMap::new();
    
    loop{
        match socket.recv_from(&mut buffer).await {
            Ok((bytes_read, src_addr)) => {
                println!("{} bytes from {}.", bytes_read, src_addr);
                
                let frag: Fragment = serde_cbor::de::from_slice(&buffer[..bytes_read]).unwrap();
                println!("Received fragment {}.", frag.frag_id);

                let st_idx = FRAG_SIZE*(frag.frag_id as usize);
                let end_idx = min(FRAG_SIZE*(frag.frag_id+1) as usize, frag.msg_len as usize);

                // if this is the first fragment create a new entry in the map
                if !map.contains_key(&frag.msg_id) {
                    let mut msg_data = vec![0; frag.msg_len as usize];

                    msg_data[st_idx..end_idx].copy_from_slice(&frag.data);

                    let mut received_frags = HashSet::new();
                    received_frags.insert(frag.frag_id);

                    let msg = BigMessage{
                        data: msg_data, 
                        msg_len: frag.msg_len, 
                        received_len: (end_idx-st_idx) as u32,
                        received_frags: received_frags
                    };

                    map.insert(frag.msg_id, msg);
                } 
                // if we received a fragment before update the message with the incoming fragment
                else {
                    // should not be needed since we know key exists
                    let _default_msg = BigMessage::default_msg(); 
                    let big_msg = map.entry(frag.msg_id).or_insert(_default_msg);

                    if !(big_msg.received_frags.contains(&frag.frag_id)) {
                        big_msg.data[st_idx..end_idx].copy_from_slice(&frag.data);
                        
                        big_msg.received_len += (end_idx-st_idx) as u32;
                        big_msg.received_frags.insert(frag.frag_id);
                    }
                    
                    if big_msg.received_len == big_msg.msg_len || (big_msg.received_len as usize / FRAG_SIZE) % BLOCK_SIZE == 0 {
                        // send ack
                        let block_id = (big_msg.received_len as usize + FRAG_SIZE * BLOCK_SIZE -1) / (FRAG_SIZE * BLOCK_SIZE) -1;
                        let ack = MSG {msg_type: Type::ACK(block_id as u32)};

                        let ack = serde_cbor::ser::to_vec(&ack).unwrap();
                        socket.send_to(&ack, "0.0.0.0:9099").await.expect("Failed to send!");
                    }
                    if big_msg.received_len == big_msg.msg_len {
                        println!("Full message is received!");
                        // save image to check if it is correct
                        let _ = fs::write("rec.jpg", &big_msg.data).await;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error receiving data: {}", e);
            }
        }
    }
}
