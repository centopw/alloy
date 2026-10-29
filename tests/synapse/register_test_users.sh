#!/bin/bash
# Register test users against local Synapse
set -e

HOMESERVER_URL="${HOMESERVER_URL:-http://localhost:8008}"
USER1="alice"
USER2="bob"
PASSWORD="testpassword123!"
# Users the user directory can find without a shared room. Keep the names stable: the screen walk
# asserts on them when it checks the palette's People results.
DIRECTORY_USERS=("carol:Carol Mendes" "dana:Dana Whitfield")

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

# Registration does not carry a display name, so set it with the account's own token. The
# user directory matches on it, which is what makes these two findable in search.
set_display_name() {
    local username=$1
    local displayname=$2
    local user_id="@$username:localhost"
    local token

    token=$(curl -s -X POST "$HOMESERVER_URL/_matrix/client/v3/login" \
        -H "Content-Type: application/json" \
        -d "{
            \"type\": \"m.login.password\",
            \"identifier\": {\"type\": \"m.id.user\", \"user\": \"$username\"},
            \"password\": \"$PASSWORD\"
        }" | python3 -c 'import json, sys; print(json.load(sys.stdin).get("access_token", ""))')

    if [ -z "$token" ]; then
        echo "Could not log in as $username; display name not set"
        return
    fi

    curl -s -X PUT "$HOMESERVER_URL/_matrix/client/v3/profile/$user_id/displayname" \
        -H "Authorization: Bearer $token" \
        -H "Content-Type: application/json" \
        -d "{\"displayname\": \"$displayname\"}" > /dev/null
    echo "Display name: $username -> $displayname"
}

for entry in "${DIRECTORY_USERS[@]}"; do
    username="${entry%%:*}"
    displayname="${entry#*:}"
    register_user "$username" "$PASSWORD"
    set_display_name "$username" "$displayname"
done

echo "Registration complete."
