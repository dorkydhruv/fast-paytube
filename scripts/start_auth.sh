#!/bin/bash

# This script is used to start the authentication service.
for i in {0..3}
do

    port=$(printf '%d00%d' $((i+8)) $i)
    cargo run -- server --config ./bridge_config/authority_$i.json --port "$port" &
    echo "Started authority server on port $port"
    sleep 10
done