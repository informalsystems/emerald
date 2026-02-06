.PHONY: all build release test docs docs-serve testnet-config testnet-reth-recreate testnet-reth-restart testnet-start sync testnet-node-stop testnet-node-restart testnet-stop testnet-clean clean-volumes clean-prometheus spam spam-contract

all: build

build:
	forge build
	cargo build
	cd custom-reth && cargo build

release:
	forge build
	cargo build --release

test:
	cargo test
	forge test -vvv

# Docs

docs:
	cd docs/operational-docs && mdbook build

docs-serve:
	cd docs/operational-docs && mdbook serve --open

# Testnet (local deployment)

RETH_NODES ?= reth0 reth1 reth2

testnet-config:
	./scripts/generate_testnet_config.sh --nodes 3 --testnet-config-dir .testnet
	cargo run --bin emerald -- testnet --home nodes --testnet-config .testnet/testnet_config.toml
	ls nodes/*/config/priv_validator_key.json | xargs -I{} cargo run --bin emerald show-pubkey {} > nodes/validator_public_keys.txt
	cargo run --bin emerald-utils genesis --public-keys-file ./nodes/validator_public_keys.txt --devnet

testnet-reth-recreate:
	docker compose down -v $(RETH_NODES)
	docker compose up -d $(RETH_NODES)
	./scripts/add_peers.sh --nodes $$(docker compose ps --status running --format "{{.Name}}" | grep -c reth)

testnet-ethrex-recreate:
	docker compose -f compose_ethrex.yaml up -d ethrex0
	./scripts/start_ethrex.sh


testnet-reth-restart:
	docker compose restart $(RETH_NODES)
	./scripts/add_peers.sh --nodes $$(docker compose ps --status running --format "{{.Name}}" | grep -c reth)

testnet-start: testnet-config testnet-reth-recreate
	docker compose up -d prometheus grafana otterscan
	@echo 👉 Grafana dashboard is available at http://localhost:4000
	bash scripts/spawn.bash --nodes 3 --home nodes --no-delay

testnet-start-ethrex: testnet-config testnet-ethrex-recreate
# 	docker compose up -d prometheus grafana otterscan
# 	@echo 👉 Grafana dashboard is available at http://localhost:4000
	bash scripts/spawn.bash --nodes 3 --home nodes --no-delay --exec-engine ethrex

sync: testnet-config testnet-reth-recreate
	docker compose up -d prometheus grafana otterscan
	@echo 👉 Grafana dashboard is available at http://localhost:4000
	cp monitoring/prometheus-syncing.yml monitoring/prometheus.yml
	docker compose restart prometheus
	bash scripts/spawn.bash --nodes $(words $(RETH_NODES)) --home nodes

NODE ?= 0# default node 0

testnet-node-stop: 
	@echo "\nStopping node $(NODE) (folder: \"nodes/$(NODE)\")"
	./scripts/kill_node.sh $(NODE)

testnet-node-restart: testnet-node-stop
	@echo "\nRestarting node $(NODE) (folder: \"nodes/$(NODE)\")"
	./scripts/restart_node.sh $(NODE)

testnet-stop:
	docker compose down

# Testnet cleanup

testnet-clean: clean-prometheus clean-volumes
	rm -rf ./.testnet
	rm -rf ./assets/genesis.json
	rm -rf ./assets/emerald_genesis.json
	rm -rf ./nodes
	rm -rf ./monitoring/data-grafana

testnet-clean-ethrex: clean-prometheus
	docker compose -f compose_ethrex.yaml down ethrex0 ethrex1 ethrex2
	rm -rf ./.testnet
	rm -rf ./assets/genesis.json
	rm -rf ./assets/emerald_genesis.json
	rm -rf ./nodes
	rm -rf ./monitoring/data-grafana
	docker volume ls --format '{{.Name}}' | grep -E 'ethrex' | xargs -r docker volume rm || true

clean-volumes:
	docker volume ls --format '{{.Name}}' | grep -E 'reth' | xargs -r docker volume rm || true

clean-prometheus: testnet-stop
	rm -rf ./monitoring/data-prometheus

# Spammer

spam:
	cargo run --bin emerald-utils spam --time=60 --rate=1000 --rpc-url=http://127.0.0.1:8645 --chain-id 12345

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
