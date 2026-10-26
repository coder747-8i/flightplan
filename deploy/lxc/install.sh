#!/usr/bin/env bash
# Install the Flightplan server on a bare Debian 12/13 system (the Proxmox LXC made by
# deploy/proxmox/flightplan-lxc.sh, or any Debian box). It installs only what Flightplan needs,
# removes what a container doesn't, and sets up:
#   flightplan.service        the web app and API on PORT (default 1826), all interfaces
#   flightplan-data.timer     every 6 h: when the FAA has a new 28-day cycle, build the chart data
#                             pack here, install it, restart, and delete the build files
#   flightplan-update <pkg>   command: update Flightplan from a new package, and Debian packages
#
# Everything is precompiled in the package (deploy/build-package.sh makes it), so nothing is
# compiled here. Run as root:
#   TARBALL=/root/flightplan-server-linux-x86_64.tar.gz bash install.sh
# (without TARBALL it looks for that file next to this script and in /root). Re-running it with a
# newer package updates Flightplan and keeps all data, passwords and settings.
# Settings (environment): PORT=1826  TUNNEL_IP=<address of the machine running the Cloudflare
#   tunnel; only its forwarded headers are trusted>
set -euo pipefail

PORT="${PORT:-1826}"
TUNNEL_IP="${TUNNEL_IP:-}"
APP=/opt/flightplan
PKG_NAME=flightplan-server-linux-x86_64.tar.gz
export DEBIAN_FRONTEND=noninteractive
# `pct exec` starts with a bare environment: a PATH without /usr/local/bin, and the Proxmox
# host's locale (which the container doesn't have)
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
export LANG=C.UTF-8 LC_ALL=C.UTF-8
unset LANGUAGE

say() { printf '\n\033[1;35m== %s\033[0m\n' "$*"; }
die() { echo "error: $*" >&2; exit 1; }

[ "$(id -u)" = 0 ] || die "run as root"
[ -r /etc/debian_version ] || die "this is for Debian"
[ "$(uname -m)" = x86_64 ] || die "the package is built for x86_64; this is $(uname -m)"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -z "${TARBALL:-}" ]; then
  for c in "$here/$PKG_NAME" "$here/../../$PKG_NAME" "/root/$PKG_NAME"; do
    [ -f "$c" ] && TARBALL="$c" && break
  done
fi
[ -n "${TARBALL:-}" ] && [ -f "$TARBALL" ] || die "no package: set TARBALL=/path/to/$PKG_NAME (make it with deploy/build-package.sh)"

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
# runtime only: certificates for HTTPS, Python for the data pipeline, SQLite/zlib/libstdc++ for
# tippecanoe. No compilers: the package is precompiled.
apt-get -y -qq install ca-certificates curl python3 python3-venv libsqlite3-0 zlib1g libstdc++6 tzdata >/dev/null

id flightplan >/dev/null 2>&1 || useradd --system --home-dir "$APP" --shell /usr/sbin/nologin flightplan

say "Flightplan"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
tar -C "$tmp" -xzf "$TARBALL"
src="$tmp/flightplan"
[ -x "$src/bin/flightplan-server" ] || die "$TARBALL isn't a Flightplan server package"
echo "version $(cat "$src/VERSION" 2>/dev/null || echo '?')"
mkdir -p "$APP"/{bin,web,data,tiles,work}
# stop first when updating, so the binary can be replaced
systemctl stop flightplan.service 2>/dev/null || true
install -m 0755 "$src/bin/flightplan-server" "$APP/bin/flightplan-server"
install -m 0755 "$src/bin/tippecanoe" /usr/local/bin/tippecanoe
rm -rf "${APP:?}/web/dist" "${APP:?}/pipeline" "${APP:?}/deploy"
cp -r "$src/web/dist" "$APP/web/dist"
cp -r "$src/pipeline" "$APP/pipeline"
cp -r "$src/deploy" "$APP/deploy"
cp "$src/VERSION" "$APP/VERSION" 2>/dev/null || true
install -m 0755 "$APP/deploy/flightplan-data-update" /usr/local/bin/flightplan-data-update
install -m 0755 "$APP/deploy/flightplan-update" /usr/local/bin/flightplan-update
if ! out="$(/usr/local/bin/tippecanoe --version 2>&1)"; then
  echo "$out" >&2
  die "tippecanoe from the package doesn't run here (is this Debian 12 or newer, x86_64?)"
fi

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
