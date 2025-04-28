group "default" {
  targets = [
    "home-arm64",
    "home-amd64",
    "home-arm64-tar",
    "home-amd64-tar",
    "home-manifest"
  ]
}

# Base target with shared settings
target "home-base" {
  target = "home" # ← default full container
  tags = ["ghcr.io/bearcove/home:latest"]
  pull = true
  labels = {
    "org.opencontainers.image.title" = "home"
    "org.opencontainers.image.source" = "https://github.com/bearcove/home"
  }
  env = {
    "DEPOT_TOKEN" = "${DEPOT_TOKEN}"
  }
}

# ARM64 container push
target "home-arm64" {
  inherits = ["home-base"]
  platforms = ["linux/arm64"]
  output = ["type=registry"]
}

# AMD64 container push
target "home-amd64" {
  inherits = ["home-base"]
  platforms = ["linux/amd64"]
  output = ["type=registry"]
}

# ARM64 tarball extraction
target "home-arm64-tar" {
  inherits = ["home-base"]
  target = "home-minimal" # 🔥 override to scratch minimal!
  platforms = ["linux/arm64"]
  output = [
    "type=tar,dest=aarch64-unknown-linux-gnu.tar.xz,compression=xz,entrypoint=/home"
  ]
  tags = [] # prevent accidentally pushing this tar image
}

# AMD64 tarball extraction
target "home-amd64-tar" {
  inherits = ["home-base"]
  target = "home-minimal" # 🔥 override to scratch minimal!
  platforms = ["linux/amd64"]
  output = [
    "type=tar,dest=x86_64-unknown-linux-gnu.tar.xz,compression=xz,entrypoint=/home"
  ]
  tags = []
}

# Manifest merging both registries
target "home-manifest" {
  type = "image"
  tags = ["ghcr.io/bearcove/home:latest"]
  platforms = ["linux/amd64", "linux/arm64"]
  output = ["type=registry"]
  inputs = ["home-amd64", "home-arm64"]
}
