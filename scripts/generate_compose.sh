#!/bin/bash

# Configuration
NODES=(0 1 2)
RPC_PORT_PREFIX=954
DISCOVERY_PORT_SUFFIX=0303
AUTH_PORT_SUFFIX="8551" #- "--authrpc.port=28551"
METRICS_PORT_PREFIX="900"
RETH_CONF="assets/reth.conf"
OUTPUT_FILE="compose.yaml"

# Start generating the compose file
cat > "$OUTPUT_FILE" << 'EOF'
volumes:
EOF

# Generate volumes section
for NODE in "${NODES[@]}"; do
    echo "  reth${NODE}:" >> "$OUTPUT_FILE"
done

# Add empty line and services section
cat >> "$OUTPUT_FILE" << 'EOF'

services:
EOF

# Generate services section for each node
for NODE in "${NODES[@]}"; do
    cat >> "$OUTPUT_FILE" << EOF
  reth${NODE}:
    network_mode: host
    image: ghcr.io/paradigmxyz/reth:v1.9.2
    container_name: reth${NODE}
    volumes:
      - reth${NODE}:/data
      - ./assets:/root/assets/
    environment:
      PATH: /usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
    command:
EOF

    # Read reth.conf and add each line as a command parameter
    if [[ -f "$RETH_CONF" ]]; then
        while IFS= read -r line || [[ -n "$line" ]]; do
            # Skip empty lines and comments
            if [[ -n "$line" ]] && [[ ! "$line" =~ ^[[:space:]]*# ]]; then
                echo "      - \"$line\"" >> "$OUTPUT_FILE"
            fi
        done < "$RETH_CONF"
    fi

    # Add the http.port parameter
    HTTP_PORT="${RPC_PORT_PREFIX}${NODE}"
    echo "      - \"--http.port=${HTTP_PORT}\"" >> "$OUTPUT_FILE"
    DISCOVERY_PORT="$((NODE+1))${DISCOVERY_PORT_SUFFIX}"
    echo "      - \"--discovery.port=${DISCOVERY_PORT}\"" >> "$OUTPUT_FILE"
    echo "      - \"--port=${DISCOVERY_PORT}\"" >> "$OUTPUT_FILE"
    AUTH_PORT="$((NODE+1))${AUTH_PORT_SUFFIX}"
    echo "      - \"--authrpc.port=${AUTH_PORT}\"" >> "$OUTPUT_FILE"
    METRICS_PORT="${METRICS_PORT_PREFIX}${NODE}"
    echo "      - \"--metrics=${METRICS_PORT}\"" >> "$OUTPUT_FILE"
done

# Add prometheus and grafana services
cat >> "$OUTPUT_FILE" << 'EOF'
  prometheus:
    network_mode: host
    build:
      dockerfile: Dockerfile.prometheus
    user: "65534"
  grafana:
    network_mode: host
    build:
      dockerfile: Dockerfile.grafana
    user: "501"
    environment:
      GF_LOG_LEVEL: info
      GF_ANALYTICS_ENABLED: false
      GF_ANALYTICS_REPORTING_ENABLED: false
      GF_ANALYTICS_CHECK_FOR_PLUGIN_UPDATES: false
      GF_ANALYTICS_CHECK_FOR_UPDATES: false
      GF_ANALYTICS_FEEDBACK_LINKS_ENABLED: false
      GF_SECURITY_DISABLE_GRAVATAR: true
      GF_DASHBOARDS_DEFAULT_HOME_DASHBOARD_PATH: /etc/grafana/provisioning/dashboards-data/default.json
      GF_USERS_DEFAULT_THEME: system
      GF_USERS_EDITORS_CAN_ADMIN: true
      GF_AUTH_ANONYMOUS_ENABLED: true
      GF_AUTH_ANONYMOUS_ORG_ROLE: Editor
      GF_AUTH_BASIC_ENABLED: false
      GF_NEWS_NEWS_FEED_ENABLED: false
  otterscan:
    image: otterscan/otterscan:develop
    depends_on:
      - reth0
    network_mode: host
    environment:
      ERIGON_URL: "http://127.0.0.1:8545"
EOF

echo "Generated $OUTPUT_FILE successfully!"
echo "Nodes: ${NODES[@]}"
echo "Port prefix: $PORT_PREFIX"
