#!/bin/bash

# Define the fixed local IP
local_ip="127.0.0.1"

# Initialize the starting port
start_port=9000

# Define the number of servers
num_servers=2

# Iterate over the number of servers and run them with incremented ports
for ((i=0; i<num_servers; i++)); do
    # Calculate the current port by incrementing the start port by 4 for each iteration
    current_port=$((start_port + i * 4))

    socket_addr="${local_ip}:${current_port}"

    log_file="client-${socket_addr}"
    
    # Run the server with the fixed IP and the calculated port
    cargo run --bin client local "$socket_addr" pokemon_reduced.txt > "$log_file" &
done

# Wait for all background processes to finish
wait
