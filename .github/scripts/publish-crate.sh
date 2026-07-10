#!/usr/bin/env bash
set -euo pipefail

crate_name="${1:?crate name is required}"
crate_version="${2:?crate version is required}"
crate_url="https://crates.io/api/v1/crates/${crate_name}/${crate_version}"
user_agent="budna-mcp-release/${crate_version} (+https://github.com/budna-marketplace/budna-mcp)"

if curl --user-agent "$user_agent" --fail --silent --show-error "$crate_url" >/dev/null; then
  echo "${crate_name} ${crate_version} is already published"
else
  cargo publish --locked --package "$crate_name"
fi

for attempt in $(seq 1 30); do
  if curl --user-agent "$user_agent" --fail --silent --show-error "$crate_url" >/dev/null; then
    echo "${crate_name} ${crate_version} is available from crates.io"
    exit 0
  fi

  echo "Waiting for ${crate_name} ${crate_version} to become available (attempt ${attempt}/30)"
  sleep 10
done

echo "Timed out waiting for ${crate_name} ${crate_version} on crates.io" >&2
exit 1
