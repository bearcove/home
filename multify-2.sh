#!/bin/bash
set -e # Exit immediately if a command exits with a non-zero status

# Parse the input tag
if [ -z "$1" ]; then
  echo -e "\033[1;31m❌ Error: IMAGE_NAME argument is required\033[0m"
  echo -e "\033[1;33mUsage: $0 <image-name:tag>\033[0m"
  exit 1
fi

IMAGE_NAME="$1"
IMAGE_BASE=$(echo $IMAGE_NAME | cut -d ':' -f 1)
TAG_VERSION=$(echo $IMAGE_NAME | cut -d ':' -f 2)

# Define architecture-specific tags
ARM64_TAG="${IMAGE_BASE}:${TAG_VERSION}-arm64"
AMD64_TAG="${IMAGE_BASE}:${TAG_VERSION}-amd64"
ARM64_FLAT_TAG="${IMAGE_BASE}:${TAG_VERSION}-arm64-flat"
AMD64_FLAT_TAG="${IMAGE_BASE}:${TAG_VERSION}-amd64-flat"

echo -e "\033[1;36m🔄 Creating multi-platform image: \033[1;32m$IMAGE_NAME\033[0m"
echo -e "\033[1;34m📦 Based on: \033[0m"
echo -e "   \033[1;35m• ARM64: \033[1;33m$ARM64_TAG\033[0m"
echo -e "   \033[1;35m• AMD64: \033[1;33m$AMD64_TAG\033[0m"

# Pull the architecture-specific images and flatten them
echo -e "\033[1;36m🔍 Pulling and flattening source images...\033[0m"

# ARM64 handling
echo -e "\033[1;34m🔄 Processing ARM64 image...\033[0m"
docker pull --platform linux/arm64 $ARM64_TAG || {
    echo -e "\033[1;31m❌ Error: Failed to pull $ARM64_TAG\033[0m";
    exit 1;
}

# Check if ARM64 is a manifest list
ARM64_MANIFEST_CHECK=$(docker manifest inspect $ARM64_TAG 2>/dev/null | grep -c "manifests")
if [ "$ARM64_MANIFEST_CHECK" -gt 0 ]; then
    echo -e "\033[1;33m⚠️  Detected ARM64 image is a manifest list, flattening...\033[0m"
    # Re-tag the pulled image to create a flat version
    docker tag $ARM64_TAG $ARM64_FLAT_TAG
    docker push $ARM64_FLAT_TAG
    echo -e "\033[1;32m✅ Pushed flattened ARM64 image: $ARM64_FLAT_TAG\033[0m"
    ARM64_TAG=$ARM64_FLAT_TAG
else
    echo -e "\033[1;32m✅ ARM64 image is already flat\033[0m"
fi

# AMD64 handling
echo -e "\033[1;34m🔄 Processing AMD64 image...\033[0m"
docker pull --platform linux/amd64 $AMD64_TAG || {
    echo -e "\033[1;31m❌ Error: Failed to pull $AMD64_TAG\033[0m";
    exit 1;
}

# Check if AMD64 is a manifest list
AMD64_MANIFEST_CHECK=$(docker manifest inspect $AMD64_TAG 2>/dev/null | grep -c "manifests")
if [ "$AMD64_MANIFEST_CHECK" -gt 0 ]; then
    echo -e "\033[1;33m⚠️  Detected AMD64 image is a manifest list, flattening...\033[0m"
    # Re-tag the pulled image to create a flat version
    docker tag $AMD64_TAG $AMD64_FLAT_TAG
    docker push $AMD64_FLAT_TAG
    echo -e "\033[1;32m✅ Pushed flattened AMD64 image: $AMD64_FLAT_TAG\033[0m"
    AMD64_TAG=$AMD64_FLAT_TAG
else
    echo -e "\033[1;32m✅ AMD64 image is already flat\033[0m"
fi

# Delete existing manifest if it exists
echo -e "\033[1;36m🗑️  Deleting existing manifest (if any)...\033[0m"
docker manifest rm $IMAGE_NAME 2>/dev/null || true

# Create the manifest
echo -e "\033[1;36m🛠️  Creating manifest with flattened images...\033[0m"
docker manifest create $IMAGE_NAME $ARM64_TAG $AMD64_TAG

# Annotate the manifest with architecture information
echo -e "\033[1;36m🏷️  Annotating manifest with architecture information...\033[0m"
docker manifest annotate $IMAGE_NAME $ARM64_TAG --os linux --arch arm64
docker manifest annotate $IMAGE_NAME $AMD64_TAG --os linux --arch amd64

# Show hashes before pushing
echo -e "\033[1;36m🔍 Manifest details:\033[0m"
MANIFEST_INFO=$(docker manifest inspect $IMAGE_NAME)
echo "$MANIFEST_INFO"

# Check if the manifest exists
if [ -z "$MANIFEST_INFO" ]; then
    echo -e "\033[1;31m❌ Error: No manifest found for $IMAGE_NAME\033[0m"
    echo -e "\033[1;33m⚠️  Please ensure that both architecture-specific images exist and are accessible.\033[0m"
    exit 1
fi

# Ask for confirmation before pushing
echo -e "\033[1;33m⚠️  Ready to push the manifest? (y/n)\033[0m"
read -r response
if [[ "$response" =~ ^([yY][eE][sS]|[yY])$ ]]; then
    echo -e "\033[1;36m📤 Pushing manifest (forced)...\033[0m"
    docker manifest push --purge $IMAGE_NAME

    # Verify the push was successful
    if [ $? -eq 0 ]; then
        echo -e "\033[1;32m✅ Multi-platform image created and pushed successfully\033[0m"
    else
        echo -e "\033[1;31m❌ Error pushing manifest. Please verify your images and registry access.\033[0m"
        echo -e "\033[1;33m⚠️  Ensure that both $ARM64_TAG and $AMD64_TAG exist and are accessible.\033[0m"
        exit 1
    fi
else
    echo -e "\033[1;31m❌ Push cancelled\033[0m"
    exit 0
fi
