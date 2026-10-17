#!/bin/bash
set -e

# Run Synapse as the invoking user so the bind-mounted data dir stays writable on Linux hosts.
export ALLOY_UID="$(id -u)" ALLOY_GID="$(id -g)"
SYNAPSE_IMAGE="matrixdotorg/synapse:v1.163.0"

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
cd "$DIR"

if [ ! -f "synapse_data/homeserver.yaml" ]; then
    echo "Generating Synapse configuration..."
    mkdir -p synapse_data
    docker run --rm -v "$DIR/synapse_data":/data \
        -e SYNAPSE_SERVER_NAME=localhost \
        -e SYNAPSE_REPORT_STATS=no \
        -e UID="$ALLOY_UID" -e GID="$ALLOY_GID" \
        "$SYNAPSE_IMAGE" generate
    
    cat << 'EOF' >> synapse_data/homeserver.yaml

enable_registration: true
enable_registration_without_verification: true

experimental_features:
  msc4186_enabled: true

rc_login:
  address:
    per_second: 1000
    burst_count: 1000
  account:
    per_second: 1000
    burst_count: 1000
  failed_attempts:
    per_second: 1000
    burst_count: 1000
rc_message:
  per_second: 1000
  burst_count: 1000
rc_registration:
  per_second: 1000
  burst_count: 1000
rc_joins:
  local:
    per_second: 1000
    burst_count: 1000
rc_room_creation:
  per_second: 1000
  burst_count: 1000
rc_invites:
  per_room:
    per_second: 1000
    burst_count: 1000
  per_user:
    per_second: 1000
    burst_count: 1000
  per_issuer:
    per_second: 1000
    burst_count: 1000
EOF
fi

echo "Starting Synapse..."
docker compose up -d

echo "Registering test users..."
./register_test_users.sh

echo "Synapse test environment ready!"
