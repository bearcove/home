group "default" {
  targets = ["home-base", "home-base-amd64", "home-base-arm64"]
}

target "home-base" {
  context = "."
  dockerfile = "Dockerfile"
  target = "home-base"
  tags = ["ghcr.io/bearcove/home-base:latest"]
  platforms = ["linux/amd64", "linux/arm64"]
  output = ["type=registry"]
}

target "home-base-amd64" {
  context = "."
  dockerfile = "Dockerfile"
  target = "home-base"
  tags = ["ghcr.io/bearcove/home-base:latest-amd64"]
  platforms = ["linux/amd64"]
  output = ["type=registry"]
}

target "home-base-arm64" {
  context = "."
  dockerfile = "Dockerfile"
  target = "home-base"
  tags = ["ghcr.io/bearcove/home-base:latest-arm64"]
  platforms = ["linux/arm64"]
  output = ["type=registry"]
}
