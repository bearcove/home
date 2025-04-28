group "default" {
  targets = [
    "home-amd64",
    "home-arm64",
    "home-manifest",
    "home-arm64-tar",
    "home-amd64-tar",
  ]
}

# Base target with shared settings
target "home-base" {
  target = "home" # ← default full container
  tags = []
  pull = true
  labels = {
    "org.opencontainers.image.title" = "home"
    "org.opencontainers.image.source" = "https://github.com/bearcove/home"
  }
  env = {
    "DEPOT_TOKEN" = "${DEPOT_TOKEN}"
  }
}

target "home-arm64" {
  inherits = ["home-base"]
  platforms = ["linux/arm64"]
  output = ["type=registry"]
  tags = ["ghcr.io/bearcove/home:arm64-latest"]
}

target "home-amd64" {
  inherits = ["home-base"]
  platforms = ["linux/amd64"]
  output = ["type=registry"]
  tags = ["ghcr.io/bearcove/home:amd64-latest"]
}

# ARM64 tarball extraction
target "home-arm64-tar" {
  target = "home-minimal" # 🔥 override to scratch minimal!
  platforms = ["linux/arm64"]
  output = [
    "type=tar,dest=aarch64-unknown-linux-gnu.tar.xz,compression=xz,entrypoint=/home"
  ]
  tags = [] # prevent accidentally pushing this tar image
}

# AMD64 tarball extraction
target "home-amd64-tar" {
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
  inputs = [
    "docker://ghcr.io/bearcove/home:amd64-latest",
    "docker://ghcr.io/bearcove/home:arm64-latest"
  ]
  depends_on = [
    "home-amd64",
    "home-arm64"
  ]
}
