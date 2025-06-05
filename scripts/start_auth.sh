#!/bin/bash
# This is for 1000 port step
# This script is used to start the authentication service.
for i in {0..3}
do

    port=$(printf '%d000' $((i+8)))
    cargo run -- server --config ./bridge_config/authority_$i.json --port "$port" &
    echo "Started authority server on port $port"
    sleep 10
done