group "default" {
  targets = ["home"]
}

target "home" {
  target = "home"
  tags = [ "ghcr.io/bearcove/home:latest" ]
  platforms = [ "linux/arm64", "linux/amd64" ]
  output = ["type=registry"]
  mounts = [ "type=cache,target=/tmp/cache" ]
  pull = true
  cache-from = ["type=registry,ref=ghcr.io/bearcove/home:cache"]
  cache-to = ["type=registry,ref=ghcr.io/bearcove/home:cache,mode=max"]
  labels = {
    "org.opencontainers.image.title" = "home"
    "org.opencontainers.image.source" = "https://github.com/bearcove/home"
  }
}
