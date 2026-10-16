#!/bin/bash
# Register test users against local Synapse
set -e

HOMESERVER_URL="${HOMESERVER_URL:-http://localhost:8008}"
USER1="alice"
USER2="bob"
PASSWORD="testpassword123!"

echo "Registering test users against Synapse at $HOMESERVER_URL"

# Wait for Synapse to be ready
echo "Waiting for Synapse to be ready..."
for i in {1..30}; do
    if curl -s "$HOMESERVER_URL/_synapse/health" > /dev/null 2>&1; then
        echo "Synapse is ready!"
        break
    fi
    echo "Attempt $i/30: waiting for Synapse..."
    sleep 2
done

register_user() {
    local username=$1
    local password=$2
    
    echo "Registering user: $username"
    
    # Try client registration endpoint first
    response=$(curl -s -X POST "$HOMESERVER_URL/_matrix/client/v3/register" \
        -H "Content-Type: application/json" \
        -d "{
            \"auth\": {\"type\": \"m.login.dummy\"},
            \"username\": \"$username\",
            \"password\": \"$password\"
        }")
    
    echo "Response: $response"
}

register_user "$USER1" "$PASSWORD"
register_user "$USER2" "$PASSWORD"

echo "Registration complete."
