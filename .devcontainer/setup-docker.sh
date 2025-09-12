#!/bin/bash
# Enhanced Docker setup script that handles group ID mapping

set -euo pipefail

echo "🔧 Setting up Docker access for dev container..."

# Check if docker socket exists
if [ ! -S /var/run/docker.sock ]; then
    echo "❌ Docker socket not found at /var/run/docker.sock"
    echo "   Make sure Docker is running on the host and the socket is mounted"
    exit 1
fi

# Get the current docker socket group ID from the host
DOCKER_SOCK_GID=$(stat -c '%g' /var/run/docker.sock)
echo "📋 Docker socket group ID: $DOCKER_SOCK_GID"

# Get the current docker group ID in the container
CONTAINER_DOCKER_GID=$(getent group docker | cut -d: -f3)
echo "📋 Container docker group ID: $CONTAINER_DOCKER_GID"

# Strategy: Instead of changing group IDs, just ensure the socket is accessible
# by setting proper permissions and group ownership

echo "🔄 Setting Docker socket permissions..."

# If socket is owned by root group (GID 0), we have two options:
if [ "$DOCKER_SOCK_GID" = "0" ]; then
    echo "📋 Docker socket is owned by root group"
    echo "🔄 Setting socket to be owned by docker group..."
    sudo chgrp docker /var/run/docker.sock
else
    echo "📋 Docker socket has custom group ID: $DOCKER_SOCK_GID"
    # Check if this GID already exists
    if getent group "$DOCKER_SOCK_GID" >/dev/null 2>&1; then
        EXISTING_GROUP=$(getent group "$DOCKER_SOCK_GID" | cut -d: -f1)
        echo "📋 GID $DOCKER_SOCK_GID belongs to group: $EXISTING_GROUP"
        echo "🔄 Adding node user to group $EXISTING_GROUP..."
        sudo usermod -aG "$EXISTING_GROUP" node
    else
        echo "🔄 Changing docker group ID to match socket..."
        sudo groupmod -g "$DOCKER_SOCK_GID" docker
    fi
fi

# Ensure the socket has proper permissions
sudo chmod g+rw /var/run/docker.sock

echo "✅ Docker socket permissions configured"

# Test Docker access
if docker ps >/dev/null 2>&1; then
    echo "🎉 Docker access verified successfully!"
    echo "📦 Docker version: $(docker --version)"
    echo "🐳 Running containers: $(docker ps --format 'table {{.Names}}\t{{.Status}}' | wc -l) container(s)"
else
    echo "❌ Docker access test failed"
    echo "🔍 Troubleshooting info:"
    echo "   Socket permissions: $(ls -la /var/run/docker.sock)"
    echo "   User groups: $(groups)"
    echo "   Docker group members: $(getent group docker)"
    exit 1
fi
