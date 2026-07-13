#!/usr/bin/env bash
set -euo pipefail

crate_name="${1:?crate name is required}"
crate_version="${2:?crate version is required}"
crate_url="https://crates.io/api/v1/crates/${crate_name}/${crate_version}"
user_agent="budna-mcp-release/${crate_version} (+https://github.com/budna-marketplace/budna-mcp)"

version_status() {
  curl \
    --user-agent "$user_agent" \
    --location \
    --silent \
    --show-error \
    --connect-timeout 10 \
    --max-time 30 \
    --retry 2 \
    --retry-delay 1 \
    --retry-connrefused \
    --output /dev/null \
    --write-out "%{http_code}" \
    "$crate_url"
}

if ! status="$(version_status)"; then
  echo "Could not determine whether ${crate_name} ${crate_version} is already published" >&2
  exit 1
fi

case "$status" in
  200)
    echo "${crate_name} ${crate_version} is already published"
    ;;
  404)
    if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
      echo "CARGO_REGISTRY_TOKEN is required to publish ${crate_name} ${crate_version}" >&2
      exit 1
    fi
    cargo publish --locked --package "$crate_name"
    ;;
  429|5??)
    echo "crates.io is temporarily unavailable while checking ${crate_name} ${crate_version} (HTTP ${status})" >&2
    exit 1
    ;;
  *)
    echo "Unexpected crates.io status while checking ${crate_name} ${crate_version}: HTTP ${status}" >&2
    exit 1
    ;;
esac

for attempt in $(seq 1 30); do
  status=""
  if status="$(version_status)" && [[ "$status" == "200" ]]; then
    echo "${crate_name} ${crate_version} is available from crates.io"
    exit 0
  fi

  if [[ -n "${status:-}" && "$status" != "404" ]]; then
    echo "crates.io returned HTTP ${status} while waiting for ${crate_name} ${crate_version}" >&2
  fi
  echo "Waiting for ${crate_name} ${crate_version} to become available (attempt ${attempt}/30)"
  sleep 10
done

echo "Timed out waiting for ${crate_name} ${crate_version} on crates.io" >&2
exit 1
