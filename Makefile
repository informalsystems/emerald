all: clean build
	./scripts/generate_testnet_config.sh --nodes 3 --testnet-config-dir .testnet
	cargo run --bin emerald -- testnet --home nodes --testnet-config .testnet/testnet_config.toml --log-level info
	ls nodes/*/config/priv_validator_key.json | xargs -I{} cargo run --bin emerald show-pubkey {} > nodes/validator_public_keys.txt
	cargo run --bin emerald-utils genesis --public-keys-file ./nodes/validator_public_keys.txt --devnet
	docker compose up -d reth0 reth1 reth2 prometheus grafana otterscan
	./scripts/add_peers.sh --nodes 3
	@echo 👉 Grafana dashboard is available at http://localhost:3000
	bash scripts/spawn.bash --nodes 3 --home nodes --no-delay

four: clean build
	./scripts/generate_testnet_config.sh --nodes 4 --testnet-config-dir .testnet
	cargo run --bin emerald -- testnet --home nodes --testnet-config .testnet/testnet_config.toml
	ls nodes/*/config/priv_validator_key.json | xargs -I{} cargo run --bin emerald show-pubkey {} > nodes/validator_public_keys.txt
	cargo run --bin emerald-utils genesis --public-keys-file ./nodes/validator_public_keys.txt --devnet
	docker compose up -d reth0 reth1 reth2 reth3 prometheus grafana otterscan
	./scripts/add_peers.sh --nodes 4
	@echo 👉 Grafana dashboard is available at http://localhost:3000
	bash scripts/spawn.bash --nodes 4 --home nodes --no-delay

sync: clean build
	./scripts/generate_testnet_config.sh --nodes 4 --testnet-config-dir .testnet
	cargo run --bin emerald -- testnet --home nodes --testnet-config .testnet/testnet_config.toml
	ls nodes/*/config/priv_validator_key.json | xargs -I{} cargo run --bin emerald show-pubkey {} > nodes/validator_public_keys.txt
	cargo run --bin emerald-utils genesis --public-keys-file ./nodes/validator_public_keys.txt --devnet
	docker compose up -d
	./scripts/add_peers.sh --nodes 4
	@echo 👉 Grafana dashboard is available at http://localhost:3000
	cp monitoring/prometheus-syncing.yml monitoring/prometheus.yml
	docker compose restart prometheus
	bash scripts/spawn.bash --nodes 4 --home nodes

build:
	forge build
	cargo build --release

stop:
	docker compose down

clean-volumes:
	docker volume ls --format '{{.Name}}' | grep -E 'reth' | xargs -r docker volume rm || true

clean: clean-prometheus clean-volumes
	rm -rf ./.testnet
	rm -rf ./assets/genesis.json
	rm -rf ./nodes
	rm -rf ./monitoring/data-grafana

clean-prometheus: stop
	rm -rf ./monitoring/data-prometheus

spam:
	cargo run --bin emerald-utils spam --time=60 --rate=5000 --rpc-url=http://127.0.0.1:8645 --chain-id 12345

spam-contract:
	@if [ -z "$(CONTRACT)" ]; then \
		echo "Error: CONTRACT address is required"; \
		echo "Usage: make spam-contract CONTRACT=0x5FbDB... FUNCTION=\"increment()\""; \
		echo "Example with args: make spam-contract CONTRACT=0x5FbDB... FUNCTION=\"setNumber(uint256)\" ARGS=\"12345\""; \
		exit 1; \
	fi; \
	if [ -z "$(FUNCTION)" ]; then \
		echo "Error: FUNCTION signature is required"; \
		echo "Usage: make spam-contract CONTRACT=0x5FbDB... FUNCTION=\"increment()\""; \
		echo "Example with args: make spam-contract CONTRACT=0x5FbDB... FUNCTION=\"setNumber(uint256)\" ARGS=\"12345\""; \
		exit 1; \
	fi; \
	cargo run --release --bin emerald-utils spam-contract \
		--contract="$(CONTRACT)" \
		--function="$(FUNCTION)" \
		--args="$(ARGS)" \
		--time=60 \
		--rate=1000 \
		--rpc-url=127.0.0.1:8645
