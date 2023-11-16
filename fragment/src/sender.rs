use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::time::{self, Duration};
use std::cmp::min;
use tokio::fs;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Fragment {
    msg_id: String,
    frag_id: u32,
    msg_len: u32,   // as bytes
    data: Vec<u8>
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
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>>{
    let socket = UdpSocket::bind("0.0.0.0:9099").await.expect("Couldn't bind to 0.0.0.0:9099");

    const BUFFER_SIZE: usize = 32768;
    const FRAG_SIZE: usize = 4096;
    const BLOCK_SIZE: usize = 2;
    const TIMEOUT_MILLIS: usize = 2000;

    let contents = fs::read("pic.jpg").await?;
    println!("Message Lenght {}", contents.len());
    // let contents = vec![1, 2, 3, 4, 5];

    let frag_num = (contents.len()+FRAG_SIZE-1) / FRAG_SIZE;    // a shorthand for ceil()
    let block_num = (frag_num+BLOCK_SIZE-1) / BLOCK_SIZE;       // a shorthand for ceil()
    let msg_len = contents.len();
    let msg_id = "1";
    
    let mut buffer = [0; BUFFER_SIZE];

    let mut curr_block = 0;
    while curr_block < block_num {

        let frag_st = BLOCK_SIZE*(usize::from(curr_block));
        let frag_end = min(BLOCK_SIZE*usize::from(curr_block+1), usize::from(frag_num));
        // construct and send fragments of the current block
        for frag_id in frag_st..frag_end {
            let st_idx = FRAG_SIZE*(usize::from(frag_id));
            let end_idx = min(FRAG_SIZE*usize::from(frag_id+1), usize::from(msg_len));

            let data_size = end_idx - st_idx;

            let mut frag_data = vec![0; data_size];

            frag_data.copy_from_slice(&contents[st_idx..end_idx]);

            let frag = Fragment{
                msg_id: String::from(msg_id), 
                frag_id: frag_id as u32, 
                msg_len: msg_len as u32, 
                data: frag_data
            };

            println!("Sending fragment {} of size {}.", frag_id, data_size);

            let frag = serde_cbor::ser::to_vec(&frag).unwrap();
            socket.send_to(&frag, "0.0.0.0:9999").await.expect("Failed to send!");
        }


        let sleep = time::sleep(Duration::from_millis(TIMEOUT_MILLIS as u64));
        tokio::pin!(sleep);

        println!("Waiting for an ACK for block {}. Timeout {} milli seconds.", curr_block, TIMEOUT_MILLIS);

        // wait for an ack
        tokio::select! {
            Ok((bytes_read, _)) = socket.recv_from(&mut buffer) => {
                // wait for the ack

                let msg: MSG = serde_cbor::de::from_slice(&buffer[..bytes_read]).unwrap();
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
    Ok(())
}
