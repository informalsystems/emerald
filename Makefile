build:
	forge build
	cargo build --release
	docker build -t emerald:latest .

spam:
	cargo run --bin malachitebft-eth-utils spam --time=60 --rate=5000 --rpc-url=127.0.0.1:8545

# Default values
NODE_COUNT ?= 3
VALIDATOR_COUNT ?= 3
BASE_HTTP_PORT ?= 8545
BASE_WS_PORT ?= 8946
BASE_ENGINE_PORT ?= 8551
BASE_METRICS_PORT ?= 9000
BASE_P2P_PORT ?= 30303
BASE_P2P_C_PORT ?= 27000
BASE_P2P_M_PORT ?= 28000
BASE_PROMETHEUS_PORT ?= 29000

GRAFANA_PORT ?= 3000
PROM_SERVER_PORT ?= 9888

.PHONY: help
help:
	@echo "Usage:"
	@echo "  make start NODE_COUNT=0"
	@echo "  make start-node NODE_ID=0"
	@echo "  make stop-node NODE_ID=0"
	@echo "  make stop-all"
	@echo "  make restart-node NODE_ID=0"
	@echo "  make reth-logs NODE_ID=0"
	@echo "  make emerald-logs NODE_ID=0"
	@echo "  make status"

.PHONY: start
.ONESHELL:
SHELL := /bin/bash
start:
	./scripts/generate_testnet_config.sh --nodes $(NODE_COUNT) --testnet-config-dir .testnet
	cargo run --bin malachitebft-eth-app -- testnet --home nodes --testnet-config .testnet/testnet_config.toml --log-level info
	ls nodes/*/config/priv_validator_key.json | xargs -I{} cargo run --bin malachitebft-eth-app show-pubkey {} > nodes/validator_public_keys.txt
	cargo run --bin malachitebft-eth-utils genesis --public-keys-file ./nodes/validator_public_keys.txt
	for i in $$(seq 0 $$(($(NODE_COUNT) - 1))); do
		HTTP_PORT=$$(($(BASE_HTTP_PORT) + $$i))
		WS_PORT=$$(($(BASE_WS_PORT) + $$i))
		P2P_PORT=$$(($(BASE_P2P_PORT) + $$i))
		ENGINE_PORT=$$(($(BASE_ENGINE_PORT) + $$i))
		METRICS_PORT=$$(($(BASE_METRICS_PORT) + $$i))
		echo "Starting reth node $$i on ports HTTP:$$HTTP_PORT WS:$$WS_PORT P2P:$$P2P_PORT ENGINE:$$ENGINE_PORT METRICS:$$METRICS_PORT"
		NODE_ID=$$i HTTP_PORT=$$HTTP_PORT WS_PORT=$$WS_PORT P2P_PORT=$$P2P_PORT ENGINE_PORT=$$ENGINE_PORT METRICS_PORT=$$METRICS_PORT \
			docker compose -p reth-node-$$i -f reth-node-compose.yaml up -d
	done
	./scripts/add_dynamic_peers.sh --nodes $(NODE_COUNT)
	for i in $$(seq 0 $$(($(NODE_COUNT) - 1))); do
		P2P_C_PORT=$$(($(BASE_P2P_C_PORT) + $$i))
		P2P_M_PORT=$$(($(BASE_P2P_M_PORT) + $$i))
		PROMETHEUS_PORT=$$(($(BASE_PROMETHEUS_PORT) + $$i))
		echo "Starting emerald node $$i on ports P2P consensus:$$P2P_C_PORT P2P mempool:$$P2P_M_PORT PROMETHEUS:$$PROMETHEUS_PORT"
		NODE_ID=$$i P2P_C_PORT=$$P2P_C_PORT P2P_M_PORT=$$P2P_M_PORT \
			docker compose -p emerald-node-$$i -f emerald-compose.yaml up -d
	done
	docker compose up -d prometheus grafana otterscan
	@echo 👉 Grafana dashboard is available at http://localhost:$(GRAFANA_PORT)
	@echo "Prometheus server is available at http://localhost:$(PROM_SERVER_PORT)"
	@echo "Otterscan is available at http://localhost:80"

.PHONY: start-node
start-node:
	@if [ -z "$(NODE_ID)" ]; then \
		echo "Error: NODE_ID required. Usage: make start-node NODE_ID=1"; \
		exit 1; \
	fi
	@echo "Starting node $(NODE_ID)..."
	NODE_ID=$(NODE_ID) \
	HTTP_PORT=$$(($(BASE_HTTP_PORT) + $(NODE_ID))) \
	WS_PORT=$$(($(BASE_WS_PORT) + $(NODE_ID))) \
	P2P_PORT=$$(($(BASE_P2P_PORT) + $(NODE_ID))) \
	ENGINE_PORT=$$(($(BASE_ENGINE_PORT) + $(NODE_ID))) \
	METRICS_PORT=$$(($(BASE_METRICS_PORT) + $(NODE_ID))) \
		docker compose -p reth-node-$(NODE_ID) -f reth-node-compose.yaml up -d;
	sleep 2
	NODE_ID=$(NODE_ID) \
	P2P_C_PORT=$$(($(BASE_P2P_C_PORT) + $(NODE_ID))) \
	P2P_M_PORT=$$(($(BASE_P2P_M_PORT) + $(NODE_ID))) \
	PROMETHEUS_PORT=$$(($(BASE_PROMETHEUS_PORT) + $(NODE_ID))) \
		docker compose -p emerald-node-$(NODE_ID) -f emerald-compose.yaml up -d;

.PHONY: stop-node
stop-node:
	@if [ -z "$(NODE_ID)" ]; then \
		echo "Error: NODE_ID required. Usage: make stop-node NODE_ID=1"; \
		exit 1; \
	fi
	@echo "Stopping node $(NODE_ID)..."
	docker compose -p reth-node-$(NODE_ID) down
	docker compose -p emerald-node-$(NODE_ID) down

.PHONY: stop-all
stop-all:
	echo "Stopping all $(NODE_COUNT) nodes..."
	for i in $$(seq 1 $(NODE_COUNT)); do
		echo "Stopping node $$i";
		docker compose -p reth-node-$$i down;
		docker compose -p emerald-node-$$i down;
	done
	docker compose down prometheus grafana otterscan

.PHONY: restart-node
restart-node:
	@if [ -z "$(NODE_ID)" ]; then \
		echo "Error: NODE_ID required. Usage: make restart-node NODE_ID=1"; \
		exit 1; \
	fi
	@echo "Restarting node $(NODE_ID)..."
	@$(MAKE) stop-node NODE_ID=$(NODE_ID)
	@$(MAKE) start-node NODE_ID=$(NODE_ID)

.PHONY: reth-logs
reth-logs:
	@if [ -z "$(NODE_ID)" ]; then \
		echo "Error: NODE_ID required. Usage: make logs NODE_ID=1"; \
		exit 1; \
	fi
	docker compose -p reth-node-$(NODE_ID) logs -f

.PHONY: emerald-logs
emerald-logs:
	@if [ -z "$(NODE_ID)" ]; then \
		echo "Error: NODE_ID required. Usage: make logs NODE_ID=1"; \
		exit 1; \
	fi
	docker compose -p emerald-node-$(NODE_ID) logs -f

.PHONY: status
status:
	@echo "Checking status of all nodes..."
	@docker ps --filter "name=reth-node-" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
	@docker ps --filter "name=emerald-node-" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"

.PHONY: clean
clean:
	rm -rf ./.testnet
	rm -rf ./assets/genesis.json
	rm -rf ./nodes
	rm -rf ./monitoring/data-grafana
	@for i in $$(seq 0 $$(($(NODE_COUNT) - 1))); do \
		docker compose -p reth-node-$$i down; \
	done
	@for i in $$(seq 0 $$(($(NODE_COUNT) - 1))); do \
		docker compose -p emerald-node-$$i down; \
	done
	@for i in $$(seq 0 $$(($(NODE_COUNT) - 1))); do \
		name="reth-node-$${i}_reth-data-$${i}"; \
		docker volume rm --force $$name 2>/dev/null || true; \
	done
