#![allow(
    dead_code,
    unused_variables,
    unused_imports,
    clippy::redundant_allocation,
    unused_assignments
)]

use client::ClientBackend;
use std::{collections::HashMap, env, io::Write, net::SocketAddr, sync::Arc};
use tokio::io::AsyncBufReadExt;
use tokio::sync::Mutex;

use crate::commons::{ENCRYPTED_PICS_PATH, LOW_RES_PICS_PATH};
use crate::utils::file_exists;

mod client;
mod commons;
mod dir_of_service;
mod encryption;
mod fragment;
mod utils;

async fn read_input() -> String {
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut buffer = Vec::new();
    let _ = reader.read_until(b'\n', &mut buffer).await;
    let input = std::str::from_utf8(&buffer[..]).unwrap().to_string();
    return String::from(input.trim());
}

async fn init() -> ClientBackend {
    // TODO: call following functions
    // register_in_DS();
    // init middleware ?? (init sockets etc)
    let args: Vec<_> = env::args().collect();
    let backend = ClientBackend::new(args).await;
    backend.init().await;
    backend
}

async fn main_menu() -> State {
    println!(
        "Welcome! Choose one of the following by typing the number.\
    \n1. Encrypt\n2. Directory of Service\n3. Edit Access\n4. View Image\n5. Quit"
    );
    loop {
        let choice = read_input().await;
        if choice == "1" {
            return State::Encryption;
        } else if choice == "2" {
            return State::DS;
        } else if choice == "3" {
            return State::EditAccess;
        } else if choice == "4" {
            return State::ViewImage;
        } else if choice == "5" {
            return State::Quit;
        }
    }
}

async fn encrypt(backend: Arc<Mutex<ClientBackend>>) -> State {
    loop {
        println!("To go back to the main menu enter m.");
        println!("Select images to encrypt by providing a path to file with image paths.");
        print!("Enter a valid path: ");
        _ = std::io::stdout().flush();
        let input = read_input().await;
        if input == "m" {
            return State::MainMenu;
        } else {
            backend.lock().await.encrypt(input.as_str()).await;
            // TODO: call encryption functionality
            // maybe we can spawn a thread for this work to keep the program interactive
            // encrypt_from_file();
        }
    }
}

async fn directory_of_service(backend: Arc<Mutex<ClientBackend>>) -> State {
    println!("To go back to the main menu enter m.");
    println!(
        "Find below the list of active users. \
    Use the index of the user to see their available images."
    );

    let dir_of_serv_map = match backend.lock().await.query_dir_of_serv().await {
        Some(d) => d,
        None => HashMap::new(),
    };

    let mut v: Vec<SocketAddr> = Vec::new();
    // let own_addr = backend
    //     .lock()
    //     .await
    //     .client_socket
    //     .clone()
    //     .local_addr()
    //     .unwrap();
    let own_addr: SocketAddr = "127.0.0.1:8091".parse().unwrap();
    let mut users_num = dir_of_serv_map.len() as u32;

    let mut idx = 0;
    for (usr, status) in &dir_of_serv_map {
        if *usr != own_addr {
            idx += 1;
            println!(
                "{}. {} | {}",
                idx,
                usr,
                if *status { "online" } else { "offline" }
            );
            v.push(*usr);
        } else {
            users_num -= 1;
        }
    }

    loop {
        print!("Enter a valid index: ");
        _ = std::io::stdout().flush();
        let input = read_input().await;
        if input == "m" {
            return State::MainMenu;
        } else {
            let idx: u32 = match input.parse() {
                Ok(num) => num,
                Err(_) => continue,
            };
            if idx > users_num || idx < 1 {
                continue;
            } else {
                let client_addr = v[(idx - 1) as usize];
                return State::ChooseImage(client_addr);
            }
        }
    }
}

async fn choose_image(backend: Arc<Mutex<ClientBackend>>, client_addr: SocketAddr) -> State {
    println!("To go back to the main menu enter m.");

    // TODO: given a user id, request the low resolution images
    backend
        .lock()
        .await
        .request_low_res_images(client_addr)
        .await;

    loop {
        print!("Enter the image name: ");
        _ = std::io::stdout().flush();
        let img_name = read_input().await;
        if img_name == "m" {
            return State::MainMenu;
        }
        if !file_exists(format!("{}/{}/{}", LOW_RES_PICS_PATH, client_addr, img_name).as_str()) {
            println!("This file does not exist!");
            continue;
        }
        // let img_id: u32 = match img_id.parse() {
        //     Ok(num) => num,
        //     Err(_) => continue,
        // };
        print!("Enter the number of requested views: ");
        _ = std::io::stdout().flush();
        let req_views = read_input().await;
        if req_views == "m" {
            return State::MainMenu;
        }
        let req_views: u32 = match req_views.parse() {
            Ok(num) => num,
            Err(_) => continue,
        };
        backend
            .lock()
            .await
            .send_image_request(img_name, req_views, client_addr)
            .await;
        return State::MainMenu;
        // TODO: request image to be viewed
        // image = request_image(image_id, req_views);
        // view_image(image);
    }
}

async fn edit_access(backend: Arc<Mutex<ClientBackend>>) -> State {
    println!("To go back to the main menu enter m.");
    println!("In front of you is a list of the images you shared. Choose an image by selecting its index.");
    let shares_num = 5;
    loop {
        print!("Enter a valid index: ");
        _ = std::io::stdout().flush();
        let input = read_input().await;
        if input == "m" {
            return State::MainMenu;
        } else {
            let idx: u32 = match input.parse() {
                Ok(num) => num,
                Err(_) => continue,
            };
            if idx > shares_num || idx < 1 {
                continue;
            } else {
                let action = get_action().await;
                if action.is_none() {
                    return State::MainMenu;
                } else {
                    // TODO: try to update access if failed send to server
                    // commit_action(action); // no need for await here
                }
            }
        }
    }
}

async fn view_image(backend: Arc<Mutex<ClientBackend>>) -> State {
    println!("To go back to the main menu enter m.");
    println!("In front of you is a list images you have access to. Use image index to select the image you wish to see.");
    let images_num = 5;
    loop {
        print!("Enter a valid source client: ");
        _ = std::io::stdout().flush();
        let mut client_dir = String::new();
        let input = read_input().await;
        if input == "m" {
            return State::MainMenu;
        } else if !file_exists(format!("{}/{}", ENCRYPTED_PICS_PATH, input).as_str()) {
            println!("You have no images from this client");
            continue;
        } else {
            client_dir = input;
            print!("Enter a valid image name: ");
            _ = std::io::stdout().flush();
            let img_name = read_input().await;
            if img_name == "m" {
                return State::MainMenu;
            }
            if !file_exists(format!("{}/{}/{}", ENCRYPTED_PICS_PATH, client_dir, img_name).as_str())
            {
                println!("This file does not exist!");
                continue;
            }

            let src_addr: SocketAddr = client_dir.parse().unwrap();
            if !backend.lock().await.view_image(img_name, src_addr).await {
                println!("No remaining views for this image");
                loop {
                    print!("Request more views (y/n)? ");
                    _ = std::io::stdout().flush();
                    let input = read_input().await;
                    if input == "m" {
                        return State::MainMenu;
                    } else if input == "n" {
                        return State::ViewImage;
                    } else if input == "y" {
                        loop {
                            print!("Enter the number of accesses: ");
                            _ = std::io::stdout().flush();
                            let input = read_input().await;
                            if input == "m" {
                                return State::MainMenu;
                            } else {
                                let val: u32 = match input.parse() {
                                    Ok(num) => num,
                                    Err(_) => continue,
                                };
                                // TODO: Make a request for the client
                                // request_extra_views(image_id, val);
                                return State::ViewImage;
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn get_action() -> Option<Action> {
    println!("Select an action.\n1. Increment number of access.\n2. Decrement number of accesses.\n3. Revoke access.");
    loop {
        print!("Enter your choice: ");
        _ = std::io::stdout().flush();
        let input = read_input().await;
        if input == "m" {
            return None;
        } else if input == "1" {
            print!("Enter increment value: ");
            _ = std::io::stdout().flush();
            let input = read_input().await;
            if input == "m" {
                return None;
            } else {
                let val: u32 = match input.parse() {
                    Ok(num) => num,
                    Err(_) => continue,
                };
                return Some(Action::Increment(val));
            }
        } else if input == "2" {
            print!("Enter decrement value: ");
            _ = std::io::stdout().flush();
            let input = read_input().await;
            if input == "m" {
                return None;
            } else {
                let val: u32 = match input.parse() {
                    Ok(num) => num,
                    Err(_) => continue,
                };
                return Some(Action::Decrement(val));
            }
        } else if input == "3" {
            return Some(Action::Revoke);
        }
    }
}
async fn quit(backend: Arc<Mutex<ClientBackend>>) {
    backend.lock().await.quit().await;
}
enum State {
    MainMenu,
    Encryption,
    DS,
    ChooseImage(SocketAddr), // user id???
    ViewImage,
    EditAccess,
    Quit,
}

enum Action {
    Increment(u32),
    Decrement(u32),
    Revoke,
}

#[tokio::main]
async fn main() {
    let backend = Arc::new(Mutex::new(init().await));
    let mut state = State::MainMenu;
    loop {
        match state {
            State::MainMenu => {
                state = main_menu().await;
            }
            State::Encryption => {
                state = encrypt(backend.clone()).await;
            }
            State::DS => {
                state = directory_of_service(backend.clone()).await;
            }
            State::ChooseImage(client_addr) => {
                state = choose_image(backend.clone(), client_addr).await;
            }
            State::EditAccess => {
                state = edit_access(backend.clone()).await;
            }
            State::ViewImage => {
                state = view_image(backend.clone()).await;
            }
            State::Quit => {
                quit(backend.clone()).await;
                break;
            }
        }
        std::process::Command::new("clear").status().unwrap();
    }
}
