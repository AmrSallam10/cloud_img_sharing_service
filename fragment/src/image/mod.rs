extern crate serde_json;

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use std::cmp::min;
// use tokio::fs;

#[derive(Serialize, Deserialize, Clone, Debug)]

mod image {
    struct Fragment {
        msg_id: String,
        frag_id: u8,
        msg_len: u16,   // as bytes
        data: Vec<u8>
    }

    #[derive(Debug)]
    pub struct BigMessage {
        data: Vec<u8>,
        msg_len: u16,
        received_len: u16,
    }

    impl BigMessage {
        pub fn from_data(data: Vec<u8>) -> Self {
            Self {
                msg_len: data.len() as u16,
                data: data,
                received_len: 0
            }
        }
        pub fn default_msg() -> Self {
            Self {
                data: vec![0;0],
                msg_len: 0,
                received_len: 0
            }
        }
        // should be converted to arc or a reference to avoid ownership issues  
        pub async fn send_fragments(self, socket: UdpSocket) {
            const FRAG_SIZE: usize = 2;
            let contents = vec![1, 2, 3, 4, 5];

            let frag_num = (contents.len()+FRAG_SIZE-1) / FRAG_SIZE;  // a shorthand for ceil()
            let msg_len = contents.len();
            let msg_id = "1";

            // construct and send fragments
            for frag_id in 0..frag_num {
                let st_idx = FRAG_SIZE*(usize::from(frag_id));
                let end_idx = min(FRAG_SIZE*usize::from(frag_id+1), usize::from(msg_len));

                let data_size = end_idx - st_idx;

                let mut frag_data = vec![0; data_size];

                frag_data.copy_from_slice(&contents[st_idx..end_idx]);

                let frag = Fragment{
                    msg_id: String::from(msg_id), 
                    frag_id: frag_id as u8, 
                    msg_len: msg_len as u16, 
                    data: frag_data
                };

                let frag = serde_json::to_string(&frag).unwrap();
                socket.send_to(&frag.as_bytes(), "0.0.0.0:9999").await.expect("Failed to send!");
            }
        }
    }
}
