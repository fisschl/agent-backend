$ErrorActionPreference = "Stop"

docker build -t agent-backend-builder .

$containerId = docker create agent-backend-builder

if (Test-Path -Path "dist") {
    Remove-Item -Path "dist\*" -Recurse -Force
} else {
    New-Item -ItemType Directory -Path "dist"
}

docker cp "${containerId}:/root/dist/." "./dist"

docker rm $containerId
