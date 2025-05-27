group "default" {
  targets = [
    "home-mom",
    "home-serve",
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

target "home-mom" {
  tags = ["ghcr.io/bearcove/home-mom:latest"]
  platforms = ["linux/amd64", "linux/arm64"]
  output = ["type=registry"]
  target = "home-mom"
  pull = true
  labels = {
    "org.opencontainers.image.title" = "home-mom"
    "org.opencontainers.image.source" = "https://github.com/bearcove/home"
  }
  env = {
    "DEPOT_TOKEN" = "${DEPOT_TOKEN}"
  }
}

target "home-serve" {
  tags = ["ghcr.io/bearcove/home-serve:latest"]
  platforms = ["linux/amd64", "linux/arm64"]
  output = ["type=registry"]
  target = "home-serve"
  pull = true
  labels = {
    "org.opencontainers.image.title" = "home-serve"
    "org.opencontainers.image.source" = "https://github.com/bearcove/home"
  }
  env = {
    "DEPOT_TOKEN" = "${DEPOT_TOKEN}"
  }
}
