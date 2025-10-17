all: clean build
	./scripts/generate_testnet_config.sh --nodes 3 --testnet-config-dir .testnet
	cargo run --bin malachitebft-eth-app -- testnet --home nodes --testnet-config .testnet/testnet_config.toml
	ls nodes/*/config/priv_validator_key.json | xargs -I{} cargo run --bin malachitebft-eth-app show-pubkey {} > nodes/validator_public_keys.txt
	cargo run --bin malachitebft-eth-utils genesis --public-keys-file ./nodes/validator_public_keys.txt
	docker compose up -d reth0 reth1 reth2 prometheus grafana otterscan
	./scripts/add_peers.sh --nodes 3
	@echo 👉 Grafana dashboard is available at http://localhost:3000
	bash scripts/spawn.bash --nodes 3 --home nodes --no-delay

sync: clean build
	./scripts/generate_testnet_config.sh --nodes 4 --testnet-config-dir .testnet
	cargo run --bin malachitebft-eth-app -- testnet --home nodes --testnet-config .testnet/testnet_config.toml
	ls nodes/*/config/priv_validator_key.json | xargs -I{} cargo run --bin malachitebft-eth-app show-pubkey {} > nodes/validator_public_keys.txt
	cargo run --bin malachitebft-eth-utils genesis --public-keys-file ./nodes/validator_public_keys.txt
	docker compose up -d
	./scripts/add_peers.sh --nodes 4
	@echo 👉 Grafana dashboard is available at http://localhost:3000
	cp monitoring/prometheus-syncing.yml monitoring/prometheus.yml
	docker compose restart prometheus
	bash scripts/spawn.bash --nodes 4 --home nodes

build:
	cargo build
	forge build

stop:
	docker compose down

clean: clean-prometheus
	rm -rf ./.testnet
	rm -rf ./assets/genesis.json
	rm -rf ./nodes
	rm -rf ./monitoring/data-grafana
	docker volume rm --force malaketh-layered-private_reth0
	docker volume rm --force malaketh-layered-private_reth1
	docker volume rm --force malaketh-layered-private_reth2
	docker volume rm --force malaketh-layered-private_reth3

clean-prometheus: stop
	rm -rf ./monitoring/data-prometheus

spam:
	cargo run --bin malachitebft-eth-utils spam --time=60 --rate=5000 --rpc-url=127.0.0.1:8545
