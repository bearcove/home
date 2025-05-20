group "default" {
  targets = [
    "home",
    "home-arm64-tar",
    "home-amd64-tar",
  ]
}

# ARM64 tarball extraction
target "home-arm64-tar" {
  target = "home-minimal"
  platforms = ["linux/arm64"]
  output = [
    "type=tar,dest=aarch64-unknown-linux-gnu.tar,entrypoint=/home"
  ]
  tags = []
}

# AMD64 tarball extraction
target "home-amd64-tar" {
  target = "home-minimal"
  platforms = ["linux/amd64"]
  output = [
    "type=tar,dest=x86_64-unknown-linux-gnu.tar,entrypoint=/home"
  ]
  tags = []
}

# Manifest merging both registries and base target settings
target "home" {
  tags = ["ghcr.io/bearcove/home:latest"]
  platforms = ["linux/amd64", "linux/arm64"]
  output = ["type=registry"]
  target = "home"
  pull = true
  labels = {
    "org.opencontainers.image.title" = "home"
    "org.opencontainers.image.source" = "https://github.com/bearcove/home"
  }
  env = {
    "DEPOT_TOKEN" = "${DEPOT_TOKEN}"
  }
}
