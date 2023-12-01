use std::io::Write;
use tokio::io::AsyncBufReadExt;

async fn read_input() -> String {
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut buffer = Vec::new();
    let _ = reader.read_until(b'\n', &mut buffer).await;
    let input = std::str::from_utf8(&buffer[..]).unwrap().to_string();
    return String::from(input.trim());
}

async fn init() {
    // register_in_DS();
    // init middleware ?? (init sockets etc)
}

async fn main_menu() -> State {
    println!("Welcome! Choose one of the following by typing the number.\
    \n1. Encrypt\n2. Directory of Service\n3. Edit Access\n4. Quit");
    loop {
        let choice = read_input().await;
        if choice == "1" {
            return State::Encryption;
        } else if choice == "2" {
            return State::DS;
        } else if choice == "3" {
            return State::EditAccess;
        } else if choice == "4" {
            return State::Quit;
        }
    }
}

async fn encrypt() -> State {
    println!("Select images to encrypt either by providing a path to file with image paths.");
    println!("To go back to the main menu enter m.");
    loop {
        print!("Enter a valid path: ");
        _ = std::io::stdout().flush();
        let input = read_input().await;
        if input == "m" {
            return State::MainMenu;
        } else {
            // encrypt_from_file();
        }
    }
}

async fn directory_of_service() -> State {
    println!("To go back to the main menu enter m.");
    println!("Find below the list of active users. \
    Use the index of the user to see their available images.");
    
    // show_users().await;

    let users_num = 5;
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
                let id = String::from("IPAddress");
                return State::ChooseImage(id);
            }
        }
    }
}

async fn choose_image(user_id: String) -> State {
    // get image from user_id and show it
    // request_low_res_images(user_id);
    println!("To go back to the main menu enter m.");
    loop {
        print!("Enter the image id: ");
        _ = std::io::stdout().flush();
        let image_id = read_input().await;
        if image_id == "m" { return State::MainMenu; }
        print!("Enter the number of requested views: ");
        _ = std::io::stdout().flush();
        let req_views = read_input().await;
        if req_views == "m" { return State::MainMenu; }
        let req_views: u32 = match req_views.parse() {
            Ok(num) => num,
            Err(_) => continue,
        };
        // image = request_image(image_id, req_views);
        // view_image(image);
    }
}

async fn edit_access() -> State {
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
                    // commit_action(action); // no need for await here
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
            if input == "m" {return None;}
            else {
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
            if input == "m" {return None;}
            else {
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
async fn quit() {
    // unregister_in_DS();
    // save important data struct
}
enum State {
    MainMenu,
    Encryption,
    DS,
    ChooseImage(String), // user id???
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
    init().await;
    let mut state = State::MainMenu;
    loop {
        match state {
            State::MainMenu => {
                state = main_menu().await;
            },
            State::Encryption => {
                state = encrypt().await;
            },
            State::DS => {
                state = directory_of_service().await;
            },
            State::ChooseImage(ref user_id) => {
                state = choose_image(user_id.clone()).await;
            }
            State::EditAccess => {
                state = edit_access().await;
            },
            State::Quit => {
                quit().await;
                break;
            },
        }
        std::process::Command::new("clear").status().unwrap();
    }

}