#!/bin/bash



# cd /Users/jasminam/Work/ethrex

echo "removing directories" 

# rm -rf node*

# rm -rf output*

echo "Starting ethrex"




# cargo run --bin ethrex -- --network /Users/jasminam/Work/emerald/assets/genesis.json --metrics  --metrics.addr "127.0.0.1" --metrics.port 9000 --ws.enabled --ws.addr "0.0.0.0" --ws.port 8646 --authrpc.jwtsecret ~/Work/emerald/assets/jwtsecret --http.port 8645 --log.level debug  --authrpc.addr "0.0.0.0" --authrpc.port 8551 --discovery.port 31303 --p2p.port 31303 --datadir node > output1.log  2>&1 &

sleep 15


ENODE1=$(curl -s http://localhost:8645 \
-X POST \
-H "Content-Type: application/json" \
--data '{"jsonrpc":"2.0","method":"admin_nodeInfo","params":[],"id":1}' \
| jq -r '.result.enode')

echo "***"

echo $ENODE1

echo "***"

# Fetch enode from URL (e.g., from reth0's JSON-RPC)                                                                                                                                                               
BOOTNODE_ENODE=$(curl -s http://localhost:8645 \
-X POST \
-H "Content-Type: application/json" \
--data '{"jsonrpc":"2.0","method":"admin_nodeInfo","params":[],"id":1}' \
| jq -r '.result.enode')

echo "Using bootnode: $BOOTNODE_ENODE"                                                                                                                                                                             
                                                                                                                                                                                                                     
  # Export and run docker-compose                                                                                                                                                                                    
export BOOTNODE_ENODE                                                                                                                                                                                              
docker compose -f compose_ethrex.yaml up -d ethrex1 

# cargo run --bin ethrex -- --network /Users/jasminam/Work/emerald/assets/genesis.json --metrics  --metrics.addr "127.0.0.1" --metrics.port 9001 --ws.enabled --ws.addr "0.0.0.0" --ws.port 8746 --authrpc.jwtsecret ~/Work/emerald/assets/jwtsecret --http.port 8745 --log.level debug --authrpc.addr "0.0.0.0" --authrpc.port 9551 --discovery.port 32303 --p2p.port 32303 --datadir node1 --bootnodes $ENODE1 > output2.log 2>&1 &

sleep 15

BOOTNODE_ENODE2=$(curl -s http://localhost:8745 \
-X POST \
-H "Content-Type: application/json" \
--data '{"jsonrpc":"2.0","method":"admin_nodeInfo","params":[],"id":1}' \
| jq -r '.result.enode')

echo $BOOTNODE_ENODE2

export BOOTNODE_ENODE2

docker compose -f compose_ethrex.yaml up -d ethrex2

# cargo run --bin ethrex -- --network /Users/jasminam/Work/emerald/assets/genesis.json --metrics  --metrics.addr "127.0.0.1" --metrics.port 9002 --ws.enabled --ws.addr "0.0.0.0" --ws.port 8846  --authrpc.jwtsecret ~/Work/emerald/assets/jwtsecret --http.port 8845 --log.level debug --authrpc.addr "0.0.0.0" --authrpc.port 10551 --discovery.port 33303 --p2p.port 33303 --datadir node2 --bootnodes $ENODE,$ENODE1 > output3.log 2>&1 &


# sleep 15

# ENODE=$(curl -s http://localhost:28645 \
# -X POST \
# -H "Content-Type: application/json" \
# --data '{"jsonrpc":"2.0","method":"admin_nodeInfo","params":[],"id":1}' \
# | jq -r '.result.enode')

# echo $ENODE

# cargo run --bin ethrex -- --network /Users/jasminam/Work/emerald/assets/genesis.json --authrpc.jwtsecret ~/Work/emerald/assets/jwtsecret --http.port 8945 --log.level debug --authrpc.port 11551 --discovery.port 34303 --p2p.port 34303 --datadir node3 --bootnodes $ENODE,$ENODE1 > output4.log 2>&1 &