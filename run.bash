#!/bin/bash

# Ensure the cargo command is available
if ! command -v cargo &> /dev/null; then
    echo "Cargo (Rust) is not installed or not in your PATH."
    exit 1
fi

# Check if the IP file exists
SERVERS_IP_FILE="/home/amrmsallam/Desktop/Fall23/Distributed_Systems/project/servers.txt"
CLIENTS_IP_FILE="/home/amrmsallam/Desktop/Fall23/Distributed_Systems/project/clients.txt"
if [ ! -f "$SERVERS_IP_FILE" ]; then
    echo "IP address file ($SERVERS_IP_FILE) not found."
    exit 1
fi

if [ ! -f "$CLIENTS_IP_FILE" ]; then
    echo "IP address file ($CLIENTS_IP_FILE) not found."
    exit 1
fi

# Compile the Rust code if necessary (you can replace 'your_server' and 'your_client' with your actual Rust binary names)
# cargo build --release --bin your_server
# cargo build --release --bin your_client

# Start your servers
while IFS= read -r ip; do
    cargo run --bin server -- "$ip" &
done < "$SERVERS_IP_FILE"

# # Start your clients
# while IFS= read -r ip; do
#     cargo run --bin client -- "$ip" &
# done < "$CLIENTS_IP_FILE"

# Sleep for a moment to ensure the servers have started before running clients
sleep 1

# Optionally, add more logic for handling server and client processes, error handling, etc.
# For instance, you can store the process IDs and wait for them to complete.
# Example:
# server_pids=()
# client_pids=()
# while IFS= read -r ip; do
#     cargo run --release --bin your_server -- --ip "$ip" &
#     server_pids+=($!)
# done < "$IP_FILE"
# sleep 1
# while IFS= read -r ip; do
#     cargo run --release --bin your_client -- --ip "$ip" &
#     client_pids+=($!)
# done < "$IP_FILE"
# # Wait for server and client processes to finish
# wait "${server_pids[@]}"
# wait "${client_pids[@]}"
