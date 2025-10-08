all: clean
	cargo build
	cargo run --bin malachitebft-eth-utils genesis
	docker compose up -d
	./scripts/add_peers.sh
	./scripts/generate_testnet_config.sh 3
	cargo run --bin malachitebft-eth-app -- testnet --home nodes --testnet-config ${HOME}/.testnet/testnet_config.toml
	echo 👉 Grafana dashboard is available at http://localhost:3000
	bash scripts/spawn.bash --nodes 3 --home nodes

stop:
	docker compose down

clean: clean-prometheus
	rm -rf ./assets/genesis.json
	rm -rf ./nodes
	rm -rf ./rethdata
	rm -rf ./monitoring/data-grafana

clean-prometheus: stop
	rm -rf ./monitoring/data-prometheus

spam:
	cargo run --bin malachitebft-eth-utils spam --time=60 --rate=500 --rpc-url=127.0.0.1:8545
