#!/usr/bin/env bash
# Build and deploy JJ Retail to a server prepared per deploy/README.md.
#
#   deploy/deploy.sh debian@15.235.247.241
#
# Env:
#   BUILD_ON  remote (default): compile the API in a Docker container on the server itself
#             (native architecture, build cache kept in Docker volumes there).
#             local: compile here in Docker for PLATFORM (slow under emulation).
#   PLATFORM  for BUILD_ON=local only; default linux/amd64.
# Needs locally: pnpm, rsync, ssh (+ Docker for BUILD_ON=local). Server: Docker for remote builds.
# The remote user needs passwordless sudo for the commands below.
# $REL and $BUILD_IMAGE are deliberately expanded locally inside ssh command strings.
# shellcheck disable=SC2029
set -euo pipefail

HOST="${1:?usage: deploy.sh user@host}"
BUILD_ON="${BUILD_ON:-remote}"
PLATFORM="${PLATFORM:-linux/amd64}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REL="$(date -u +%Y%m%d%H%M%S)-$(git -C "$ROOT" rev-parse --short HEAD)"
BUILD_IMAGE="rust:1-trixie"

if [ -n "$(git -C "$ROOT" status --porcelain)" ]; then
  echo "!! working tree has uncommitted changes; commit them first so the release matches $REL" >&2
  exit 1
fi

ssh "$HOST" "mkdir -p /tmp/jj-$REL/web ~/jj-build/backend"

echo "==> Building API ($BUILD_ON)"
if [ "$BUILD_ON" = remote ]; then
  rsync -az --delete --exclude target/ --exclude .env "$ROOT/backend/" "$HOST:jj-build/backend/"
  ssh "$HOST" "sudo docker run --rm \
    -v \$HOME/jj-build/backend:/src -v jj-cargo-registry:/usr/local/cargo/registry -v jj-cargo-target:/target \
    -v /tmp/jj-$REL:/out -w /src -e SQLX_OFFLINE=true -e CARGO_TARGET_DIR=/target \
    $BUILD_IMAGE sh -c 'cargo build --release --locked && cp /target/release/jj-retail-api /out/'"
else
  ARCH="${PLATFORM#linux/}"
  docker run --rm --platform "$PLATFORM" \
    -v "$ROOT/backend":/src -w /src \
    -e SQLX_OFFLINE=true -e CARGO_TARGET_DIR="/src/target/linux-$ARCH" \
    "$BUILD_IMAGE" cargo build --release --locked
  rsync -az "$ROOT/backend/target/linux-$ARCH/release/jj-retail-api" "$HOST:/tmp/jj-$REL/"
fi

echo "==> Building web app"
(cd "$ROOT/frontend" && pnpm install --frozen-lockfile && pnpm build)

echo "==> Uploading release $REL"
rsync -az --delete "$ROOT/frontend/dist/" "$HOST:/tmp/jj-$REL/web/"

echo "==> Activating on server"
# shellcheck disable=SC2087 # $REL is meant to expand locally
ssh "$HOST" bash -s <<REMOTE
set -euo pipefail
REL="$REL"
sudo install -d -o root -g root /opt/jj-retail/releases/\$REL /srv/jj-retail/web-releases/\$REL
sudo install -m 0755 /tmp/jj-\$REL/jj-retail-api /opt/jj-retail/releases/\$REL/
sudo cp -a /tmp/jj-\$REL/web/. /srv/jj-retail/web-releases/\$REL/
rm -rf /tmp/jj-\$REL

PREV_API="\$(readlink /opt/jj-retail/current || true)"
PREV_WEB="\$(readlink /srv/jj-retail/web || true)"

echo "-- migrations (must be backward compatible with the running release)"
# systemd-run reads /etc/jj-retail/env exactly as the service does (EnvironmentFile).
sudo systemd-run --quiet --wait --pipe --collect --uid=jj-retail --gid=jj-retail \
  -p EnvironmentFile=/etc/jj-retail/env /opt/jj-retail/releases/\$REL/jj-retail-api migrate

sudo ln -sfn /opt/jj-retail/releases/\$REL /opt/jj-retail/current
sudo ln -sfn /srv/jj-retail/web-releases/\$REL /srv/jj-retail/web
sudo systemctl restart jj-retail-api

for _ in \$(seq 1 20); do
  if curl -fsS http://127.0.0.1:8000/health/ready >/dev/null; then
    echo "-- healthy: \$REL"
    # Keep the five newest releases.
    ls -1dt /opt/jj-retail/releases/* | tail -n +6 | xargs -r sudo rm -rf
    ls -1dt /srv/jj-retail/web-releases/* | tail -n +6 | xargs -r sudo rm -rf
    exit 0
  fi
  sleep 1
done

echo "!! health check failed; rolling back" >&2
if [ -n "\$PREV_API" ]; then sudo ln -sfn "\$PREV_API" /opt/jj-retail/current; fi
if [ -n "\$PREV_WEB" ]; then sudo ln -sfn "\$PREV_WEB" /srv/jj-retail/web; fi
sudo systemctl restart jj-retail-api
sudo journalctl -u jj-retail-api -n 50 --no-pager >&2
exit 1
REMOTE
echo "==> Done"
