group "default" {
  targets = []
}

target "home-aarch64-unknown-linux-gnu" {
  context = "."
  dockerfile = "Dockerfile"
  target = "home"
  tags = ["ghcr.io/bearcove/home:latest-arm64"]
  platforms = ["linux/arm64"]
  output = ["type=registry"]
  mounts = [
    "type=cache,target=/tmp/cache"
  ]
  env = {
    GITHUB_REF             = "${GITHUB_REF}"
    GITHUB_SERVER_URL      = "${GITHUB_SERVER_URL}"
    BEARDIST_ARTIFACT_NAME = "aarch64-unknown-linux-gnu"
    # force timelord sync
    CI                     = "1"
  }
}

target "home-x86_64-unknown-linux-gnu" {
  context = "."
  dockerfile = "Dockerfile"
  target = "home"
  tags = ["ghcr.io/bearcove/home:latest-amd64"]
  platforms = ["linux/amd64"]
  output = ["type=registry"]
  mounts = [
    "type=cache,target=/tmp/cache"
  ]
  env = {
    GITHUB_REF             = "${GITHUB_REF}"
    GITHUB_SERVER_URL      = "${GITHUB_SERVER_URL}"
    BEARDIST_ARTIFACT_NAME = "x86_64-unknown-linux-gnu"
    # force timelord sync
    CI                     = "1"
  }
}

target "home-multiarch" {
  depends_on = ["home-aarch64-unknown-linux-gnu", "home-x86_64-unknown-linux-gnu"]
  platforms = [
    "linux/arm64",
    "linux/amd64"
  ]
  tags = [
    "ghcr.io/bearcove/home:latest",
    "${EXTRA_TAG}"
  ]
  # Import images built previously for both platforms
  sources = [
    "ghcr.io/bearcove/home:latest-arm64",
    "ghcr.io/bearcove/home:latest-amd64"
  ]
  output = ["type=registry"]
}
