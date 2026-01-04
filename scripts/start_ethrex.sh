#!/bin/bash



cd /Users/jasminam/Work/ethrex

echo "Starting ethrex"

cargo run --bin ethrex -- --network /Users/jasminam/Work/emerald/assets/genesis.json --authrpc.jwtsecret ~/Work/emerald/assets/jwtsecret --http.port 8645 --log.level debug --authrpc.port 8551 --discovery.port 31303 --p2p.port 31303 --datadir node > output1.log  2>&1 &

sleep 5


ENODE=$(curl -s http://localhost:8645 \
-X POST \
-H "Content-Type: application/json" \
--data '{"jsonrpc":"2.0","method":"admin_nodeInfo","params":[],"id":1}' \
| jq '.result.enode')

echo "***"

echo $ENODE

echo "***"

cargo run --bin ethrex -- --network /Users/jasminam/Work/emerald/assets/genesis.json --authrpc.jwtsecret ~/Work/emerald/assets/jwtsecret --http.port 8745 --log.level debug --authrpc.port 9551 --discovery.port 32303 --p2p.port 32303 --datadir node1 --bootnodes $ENODE > output2.log 2>&1 &

sleep 5

ENODE=$(curl -s http://localhost:18645 \
-X POST \
-H "Content-Type: application/json" \
--data '{"jsonrpc":"2.0","method":"admin_nodeInfo","params":[],"id":1}' \
| jq '.result.enode')

echo $ENODE

cargo run --bin ethrex -- --network /Users/jasminam/Work/emerald/assets/genesis.json --authrpc.jwtsecret ~/Work/emerald/assets/jwtsecret --http.port 8845 --log.level debug --authrpc.port 10551 --discovery.port 33303 --p2p.port 33303 --datadir node2 --bootnodes $ENODE > output3.log 2>&1 &


sleep 5

ENODE=$(curl -s http://localhost:28645 \
-X POST \
-H "Content-Type: application/json" \
--data '{"jsonrpc":"2.0","method":"admin_nodeInfo","params":[],"id":1}' \
| jq '.result.enode')

echo $ENODE

cargo run --bin ethrex -- --network /Users/jasminam/Work/emerald/assets/genesis.json --authrpc.jwtsecret ~/Work/emerald/assets/jwtsecret --http.port 8945 --log.level debug --authrpc.port 11551 --discovery.port 34303 --p2p.port 34303 --datadir node3 --bootnodes $ENODE > output4.log 2>&1 &