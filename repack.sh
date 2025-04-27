#!/usr/bin/env bash
set -euo pipefail

# This script creates a container image where each file is stored in its own layer
# This approach allows for better layer caching and more granular control
# We're limited to 128 layers, but our file count is well below that threshold
# The process involves:
#  1. Creating an OCI layout directory for each file
#  2. Adding each file as a separate layer to the image
#  3. Pushing the final image with all layers

if [ "${IMAGE_PLATFORM}" != "linux/arm64" ] && [ "${IMAGE_PLATFORM}" != "linux/amd64" ]; then
    echo "IMAGE_PLATFORM must be set to linux/arm64 or linux/amd64"
    exit 1
fi

ARCH_NAME=$(echo "${IMAGE_PLATFORM}" | cut -d'/' -f2)

# Check if we're on a tag and get the version
TAG_VERSION=""
if [[ "${GITHUB_REF:-}" == refs/tags/* ]]; then
  TAG_VERSION="${GITHUB_REF#refs/tags/}"
  # Remove 'v' prefix if present
  TAG_VERSION="${TAG_VERSION#v}"
  echo -e "\033[1;33m📦 Detected tag: ${TAG_VERSION}\033[0m"
fi

# Declare variables
OCI_LAYOUT_DIR="/tmp/beardist-oci-layout"
OUTPUT_DIR="/tmp/beardist-output"
IMAGE_NAME="ghcr.io/bearcove/home:${TAG_VERSION}-${ARCH_NAME}"
BASE_IMAGE="ghcr.io/bearcove/home-base:latest-${ARCH_NAME}"

# Set the version for home-drawio
HOME_DRAWIO_VERSION="v1.0.1"

# Check if GH_READWRITE_TOKEN is set
if [ -z "${GH_READWRITE_TOKEN}" ]; then
    echo -e "\033[1;31m❌ Error: GH_READWRITE_TOKEN is not set\033[0m" >&2
    exit 1
fi

# Create a temporary directory for downloading
TEMP_DIR=$(mktemp -d)

# Download the archive
echo -e "\033[1;34m📥 Downloading home-drawio \033[1;33m${HOME_DRAWIO_VERSION}\033[0m for \033[1;36m${ARCH_NAME}\033[0m..."

# Map platform architecture to package architecture string
if [ "${ARCH_NAME}" == "amd64" ]; then
    PKG_ARCH="x86_64-unknown-linux-gnu"
elif [ "${ARCH_NAME}" == "arm64" ]; then
    PKG_ARCH="aarch64-unknown-linux-gnu"
else
    echo -e "\033[1;31m❌ Error: Unsupported architecture: ${ARCH_NAME}\033[0m" >&2
    exit 1
fi

curl --fail --location --retry 3 --retry-delay 5 -H "Authorization: token ${GH_READWRITE_TOKEN}" \
    "https://github.com/bearcove/home-drawio/releases/download/${HOME_DRAWIO_VERSION}/${PKG_ARCH}.tar.xz" \
    -o "${TEMP_DIR}/home-drawio.tar.xz"

# Unpack the archive directly to OUTPUT_DIR
echo -e "\033[1;34m📦 Unpacking home-drawio to \033[1;35m${OUTPUT_DIR}\033[0m..."
tar -xJf "${TEMP_DIR}/home-drawio.tar.xz" -C "${OUTPUT_DIR}"

# Clean up the temporary directory
rm -rf "${TEMP_DIR}"

# Check if the home-drawio binary exists and is executable
if [ ! -x "${OUTPUT_DIR}/home-drawio" ]; then
    echo -e "\033[1;31m❌ Error: home-drawio binary not found or not executable\033[0m" >&2
    exit 1
fi

# Execute home-drawio to verify it works
echo -e "\033[1;34m🧪 Testing home-drawio...\033[0m"
if ! "${OUTPUT_DIR}/home-drawio" --help; then
    echo -e "\033[1;31m❌ Error: home-drawio failed to execute\033[0m" >&2
    exit 1
fi

# Initialize the image from the base
echo -e "\033[1;36m🔄 Creating initial image from base\033[0m"
regctl image mod $BASE_IMAGE --create $IMAGE_NAME

# Initialize an array to store layer-add arguments
layer_add_args=()

# Clean up layout directory once at the beginning
rm -rf "$OCI_LAYOUT_DIR"
mkdir -p "$OCI_LAYOUT_DIR"

# Process each file as a separate layer
layer_count=0
for file in "$OUTPUT_DIR"/*; do
    if [[ -f "$file" ]]; then
        filename=$(basename "$file")
        echo -e "\033[1;33m📦 Adding file as layer: \033[1;35m$filename\033[0m"

        # Create a numbered subdirectory for each layer
        layer_dir="$OCI_LAYOUT_DIR/layer_$((++layer_count))"
        mkdir -p "$layer_dir"

        # Copy file to the appropriate directory based on extension
        if [[ "$filename" == *.so ]]; then
            mkdir -p "$layer_dir/usr/libexec"
            cp -v "$file" "$layer_dir/usr/libexec/"
            target_file="$layer_dir/usr/libexec/$filename"
        else
            mkdir -p "$layer_dir/usr/bin"
            cp -v "$file" "$layer_dir/usr/bin/"
            target_file="$layer_dir/usr/bin/$filename"
        fi

        # Reset all timestamps to epoch
        touch -t 197001010000.00 "$target_file"

        # Add layer-add argument to the array
        layer_add_args+=("--layer-add" "dir=$layer_dir")
    fi
done

# Add all layers in a single regctl command
echo -e "\033[1;36m🔄 Adding all layers to the image\033[0m"
start_time=$(date +%s)
regctl image mod $IMAGE_NAME --create $IMAGE_NAME "${layer_add_args[@]}"
end_time=$(date +%s)
duration=$((end_time - start_time))
echo -e "\033[1;33m⏱️ Adding layers took \033[1;35m$duration\033[0m seconds"

# Push the image
echo -e "\033[1;32m🚀 Pushing image: \033[1;35m$IMAGE_NAME\033[0m"
regctl image copy $IMAGE_NAME{,}

# Test the image if not in CI
if [ -z "${CI:-}" ]; then
    echo -e "\033[1;34m🧪 Testing image locally\033[0m"
    docker pull $IMAGE_NAME
    docker run --rm $IMAGE_NAME home doctor

    # Display image info
    echo -e "\033[1;35m📋 Image layer information:\033[0m"
    docker image inspect $IMAGE_NAME --format '{{.RootFS.Layers | len}} layers'
fi
