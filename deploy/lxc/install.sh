#!/usr/bin/env bash
# Install the Flightplan server on a bare Debian 12/13 system (the Proxmox LXC made by
# deploy/proxmox/flightplan-lxc.sh, or any Debian box). It installs only what Flightplan needs,
# removes what a container doesn't, and sets up:
#   flightplan.service        the web app and API on PORT (default 1826), all interfaces
#   flightplan-data.timer     every 6 h: when the FAA has a new 28-day cycle, build the chart data
#                             pack here, install it, restart, and delete the build files
#   flightplan-update         command: update Flightplan and Debian packages
#
# Run as root:  bash install.sh
# Settings (environment): PORT=1826  TUNNEL_IP=<address of the machine running the Cloudflare
#   tunnel; only its forwarded headers are trusted>  REPO=coder747-8i/flightplan  REF=main
#   FROM_SOURCE=1 (build the server here instead of downloading the release; needs ~2 GB RAM)
#   TARBALL=/path/to/flightplan-server-linux-x86_64.tar.gz (install from a file you have)
set -euo pipefail

PORT="${PORT:-1826}"
REPO="${REPO:-coder747-8i/flightplan}"
REF="${REF:-main}"
TUNNEL_IP="${TUNNEL_IP:-}"
APP=/opt/flightplan
RELEASE_URL="https://github.com/$REPO/releases/download/server/flightplan-server-linux-x86_64.tar.gz"
export DEBIAN_FRONTEND=noninteractive

say() { printf '\n\033[1;35m== %s\033[0m\n' "$*"; }
die() { echo "error: $*" >&2; exit 1; }

[ "$(id -u)" = 0 ] || die "run as root"
[ -r /etc/debian_version ] || die "this is for Debian"
[ "$(uname -m)" = x86_64 ] || [ "${FROM_SOURCE:-}" = 1 ] || die "the prebuilt server is x86_64 only; on $(uname -m) run with FROM_SOURCE=1"

say "Trimming the system"
# a container needs none of these; journald replaces rsyslog, systemd timers replace cron
for p in postfix exim4 exim4-base exim4-config exim4-daemon-light bsd-mailx rsyslog cron logrotate man-db manpages groff-base info install-info \
  apt-listchanges bash-completion vim-tiny nano whiptail ssh openssh-server openssh-client; do
  dpkg -s "$p" >/dev/null 2>&1 && apt-get -y -qq purge "$p" >/dev/null 2>&1 || true
done
# no man pages, docs or translations from here on
cat >/etc/dpkg/dpkg.cfg.d/01-flightplan-nodoc <<'EOF'
path-exclude /usr/share/doc/*
path-include /usr/share/doc/*/copyright
path-exclude /usr/share/man/*
path-exclude /usr/share/info/*
path-exclude /usr/share/locale/*
path-include /usr/share/locale/locale.alias
EOF
cat >/etc/apt/apt.conf.d/01-flightplan-minimal <<'EOF'
APT::Install-Recommends "false";
APT::Install-Suggests "false";
EOF
mkdir -p /etc/systemd/journald.conf.d
printf '[Journal]\nSystemMaxUse=50M\n' >/etc/systemd/journald.conf.d/flightplan.conf
systemctl restart systemd-journald 2>/dev/null || true

say "Packages"
apt-get -qq update
# runtime: certificates for HTTPS, Python for the data pipeline, SQLite/zlib for tippecanoe
apt-get -y -qq install ca-certificates curl python3 python3-venv libsqlite3-0 zlib1g tzdata >/dev/null
BUILD_DEPS="g++ make libsqlite3-dev zlib1g-dev"

id flightplan >/dev/null 2>&1 || useradd --system --home-dir "$APP" --shell /usr/sbin/nologin flightplan

say "Flightplan"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
if [ -n "${TARBALL:-}" ]; then
  tar -C "$tmp" -xzf "$TARBALL"
  src="$tmp/flightplan"
elif [ "${FROM_SOURCE:-}" != 1 ] && curl -fsSL -o "$tmp/fp.tar.gz" "$RELEASE_URL"; then
  tar -C "$tmp" -xzf "$tmp/fp.tar.gz"
  src="$tmp/flightplan"
else
  echo "building from source ($REPO@$REF): takes a while and ~2 GB of memory"
  apt-get -y -qq install $BUILD_DEPS pkg-config >/dev/null
  curl -fsSL "https://codeload.github.com/$REPO/tar.gz/$REF" | tar -C "$tmp" -xz
  repo="$(echo "$tmp"/flightplan-*)"
  # toolchains live in the temporary folder and go away with it
  export RUSTUP_HOME="$tmp/rustup" CARGO_HOME="$tmp/cargo" CARGO_BUILD_JOBS=2
  curl -fsSL https://sh.rustup.rs | sh -s -- -y -q --profile minimal --no-modify-path >/dev/null
  node_ver=v22.12.0
  curl -fsSL "https://nodejs.org/dist/$node_ver/node-$node_ver-linux-x64.tar.gz" | tar -C "$tmp" -xz
  export PATH="$CARGO_HOME/bin:$tmp/node-$node_ver-linux-x64/bin:$PATH"
  (cd "$repo/web" && npm ci --no-audit --no-fund --loglevel=error && npm run build >/dev/null)
  (cd "$repo/server" && cargo build --release --locked -q)
  src="$tmp/pkg"
  mkdir -p "$src/bin" "$src/web"
  cp "$repo/server/target/release/flightplan-server" "$src/bin/"
  strip "$src/bin/flightplan-server" 2>/dev/null || true
  cp -r "$repo/web/dist" "$src/web/dist"
  cp -r "$repo/pipeline" "$src/pipeline"
  rm -rf "$src/pipeline/.tools"
  cp -r "$repo/deploy/lxc" "$src/deploy"
  echo "$REF (source)" >"$src/VERSION"
fi
mkdir -p "$APP"/{bin,web,data,tiles,work}
install -m 0755 "$src/bin/flightplan-server" "$APP/bin/flightplan-server"
rm -rf "$APP/web/dist" "$APP/pipeline" "$APP/deploy"
cp -r "$src/web/dist" "$APP/web/dist"
cp -r "$src/pipeline" "$APP/pipeline"
cp -r "$src/deploy" "$APP/deploy"
cp "$src/VERSION" "$APP/VERSION" 2>/dev/null || true
install -m 0755 "$APP/deploy/flightplan-data-update" /usr/local/bin/flightplan-data-update
install -m 0755 "$APP/deploy/flightplan-update" /usr/local/bin/flightplan-update

say "tippecanoe (builds the chart tiles)"
if ! command -v tippecanoe >/dev/null; then
  apt-get -y -qq install $BUILD_DEPS >/dev/null
  curl -fsSL https://codeload.github.com/felt/tippecanoe/tar.gz/refs/heads/main | tar -C "$tmp" -xz
  # two compilers at most: each takes a few hundred MB, and a small container has 1 GB
  make -C "$tmp"/tippecanoe-main -j2 tippecanoe >/dev/null 2>"$tmp/tc.log" || { tail -5 "$tmp/tc.log"; die "building tippecanoe failed"; }
  install -m 0755 "$tmp"/tippecanoe-main/tippecanoe /usr/local/bin/tippecanoe
fi
# compilers aren't needed at run time
apt-get -y -qq purge $BUILD_DEPS pkg-config >/dev/null 2>&1 || true
apt-get -y -qq autoremove --purge >/dev/null 2>&1 || true

say "Python environment for the data pipeline"
[ -x "$APP/work/.venv/bin/python" ] || python3 -m venv "$APP/work/.venv"
"$APP/work/.venv/bin/python" -m pip install -q --no-cache-dir --upgrade "duckdb<2" shapely

chown -R flightplan:flightplan "$APP"
chmod 0750 "$APP/data"

say "Services"
if [ -n "$TUNNEL_IP" ]; then proxy="Environment=FP_TRUSTED_PROXIES=$TUNNEL_IP"; else proxy="Environment=FP_TRUST_PROXY=1"; fi
cat >/etc/systemd/system/flightplan.service <<EOF
[Unit]
Description=Flightplan (flight planner web app)
After=network-online.target
Wants=network-online.target

[Service]
User=flightplan
Group=flightplan
WorkingDirectory=$APP
Environment=FP_ROOT=$APP
Environment=PORT=$PORT
Environment=FP_BIND=0.0.0.0
# behind a Cloudflare tunnel: real visitor addresses and HTTPS come from its headers
$proxy
# fewer malloc arenas: a much smaller memory footprint for a small, mostly idle server
Environment=MALLOC_ARENA_MAX=2
Environment=RUST_LOG=warn
ExecStart=$APP/bin/flightplan-server
Restart=on-failure
RestartSec=3
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=$APP/data $APP/tiles
ProtectKernelTunables=yes
ProtectControlGroups=yes
RestrictSUIDSGID=yes
LockPersonality=yes

[Install]
WantedBy=multi-user.target
EOF
cat >/etc/systemd/system/flightplan-data.service <<EOF
[Unit]
Description=Flightplan: build and install new FAA chart data when a cycle starts
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/flightplan-data-update
Nice=10
IOSchedulingClass=idle
EOF
cat >/etc/systemd/system/flightplan-data.timer <<'EOF'
[Unit]
Description=Check for new FAA chart data every 6 hours

[Timer]
OnBootSec=10min
OnCalendar=*-*-* 00/6:17:00
RandomizedDelaySec=20min
Persistent=true

[Install]
WantedBy=timers.target
EOF
systemctl daemon-reload
systemctl enable -q --now flightplan.service flightplan-data.timer
systemctl restart flightplan.service

up() {
  for _ in $(seq 1 30); do curl -fs "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1 && return 0; sleep 1; done
  return 1
}
if ! up && journalctl -u flightplan -n 30 --no-pager 2>/dev/null | grep -qE '226/NAMESPACE|namespac'; then
  # some container setups can't give a service its own mount namespace: run without that sandbox
  echo "this container can't sandbox the service; running it without the filesystem sandbox"
  mkdir -p /etc/systemd/system/flightplan.service.d
  printf '[Service]\nPrivateTmp=no\nProtectSystem=no\nProtectHome=no\nProtectKernelTunables=no\nProtectControlGroups=no\nReadWritePaths=\n' \
    >/etc/systemd/system/flightplan.service.d/10-no-sandbox.conf
  systemctl daemon-reload
  systemctl restart flightplan.service
fi
up || { journalctl -u flightplan -n 20 --no-pager; die "the server didn't start"; }

# first chart data, in the background (about 10 to 20 minutes)
if [ ! -f "$APP/data/build/manifest.json" ]; then
  systemctl start --no-block flightplan-data.service
fi

rm -rf /var/lib/apt/lists/* /var/cache/apt/*.bin
ip="$(hostname -I 2>/dev/null | awk '{print $1}')"
say "Done"
echo "Flightplan is running:  http://${ip:-<this machine>}:$PORT"
if [ -f "$APP/data/setup-code.txt" ]; then
  echo "Setup code (enter it in the browser to set your password):  $(cat "$APP/data/setup-code.txt")"
fi
echo "Chart data is building in the background: journalctl -fu flightplan-data"
echo "Cloudflare tunnel: point a public hostname at http://${ip:-<this machine>}:$PORT"
