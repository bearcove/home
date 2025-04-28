group "default" {
  targets = ["home"]
}

target "home" {
  target = "home"
  tags = [ "ghcr.io/bearcove/home:latest" ]
  platforms = [ "linux/arm64", "linux/amd64" ]
  output = ["type=registry"]
  pull = true
  labels = {
    "org.opencontainers.image.title" = "home"
    "org.opencontainers.image.source" = "https://github.com/bearcove/home"
  }
  env = {
    "DEPOT_TOKEN" = "${DEPOT_TOKEN}"
  }
}
