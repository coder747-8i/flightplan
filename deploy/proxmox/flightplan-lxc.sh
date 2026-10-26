#!/usr/bin/env bash
# Create a Flightplan container on Proxmox VE: a bare, unprivileged Debian 12 LXC that runs only
# the Flightplan server (port 1826 on its LAN address) and rebuilds the FAA chart data itself
# every 28-day cycle. Everything comes from the server package (deploy/build-package.sh): copy
# it to the Proxmox host, unpack it, and run this as root:
#
#   scp dist/flightplan-server-linux-x86_64.tar.gz root@proxmox:
#   ssh root@proxmox
#   tar xzf flightplan-server-linux-x86_64.tar.gz && bash flightplan/deploy/flightplan-lxc.sh
#
# Update an existing container from a newer package (same steps, then):
#   CTID=<id> UPDATE=1 bash flightplan/deploy/flightplan-lxc.sh
#
# Settings (environment, all optional):
#   CTID=<next free>  CT_HOSTNAME=flightplan  CORES=2  RAM=1024 (MB)  SWAP=512 (MB)  DISK=8 (GB)
#   STORAGE=<auto: local-lvm, else the first that takes containers>  TEMPLATE_STORAGE=local
#   BRIDGE=vmbr0  IP=dhcp (or 192.168.1.50/24 with GW=192.168.1.1)  VLAN=<tag>
#   PORT=1826  TUNNEL_IP=<LAN address of the machine running cloudflared>  YES=1 (don't ask)
#   TARBALL=<the package; default: found next to this script>
set -euo pipefail

PKG_NAME=flightplan-server-linux-x86_64.tar.gz
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -z "${TARBALL:-}" ]; then
  for c in "$here/../../$PKG_NAME" "$here/../$PKG_NAME" "$PWD/$PKG_NAME" "/root/$PKG_NAME"; do
    [ -f "$c" ] && TARBALL="$(cd "$(dirname "$c")" && pwd)/$PKG_NAME" && break
  done
fi
# not $HOSTNAME: bash sets that to the Proxmox host's own name
CT_HOSTNAME="${CT_HOSTNAME:-flightplan}"
CORES="${CORES:-2}"
RAM="${RAM:-1024}"
SWAP="${SWAP:-512}"
DISK="${DISK:-8}"
BRIDGE="${BRIDGE:-vmbr0}"
IP="${IP:-dhcp}"
PORT="${PORT:-1826}"
TEMPLATE_STORAGE="${TEMPLATE_STORAGE:-local}"

say() { printf '\n\033[1;35m== %s\033[0m\n' "$*"; }
die() { echo "error: $*" >&2; exit 1; }

[ "$(id -u)" = 0 ] || die "run as root on the Proxmox host"
command -v pct >/dev/null && command -v pveam >/dev/null || die "this runs on a Proxmox VE host (pct and pveam not found)"
[ -n "${TARBALL:-}" ] && [ -f "$TARBALL" ] || die "no package: put $PKG_NAME next to the flightplan folder or set TARBALL=..."

# push the package into a container and run its installer
install_into() {
  local id="$1"
  pct push "$id" "$TARBALL" "/root/$PKG_NAME"
  pct exec "$id" -- env PORT="$PORT" TUNNEL_IP="${TUNNEL_IP:-}" TARBALL="/root/$PKG_NAME" \
    bash -c "tar -C /tmp -xzf /root/$PKG_NAME flightplan/deploy/install.sh && bash /tmp/flightplan/deploy/install.sh; s=\$?; rm -rf /tmp/flightplan /root/$PKG_NAME; exit \$s"
}

if [ "${UPDATE:-}" = 1 ]; then
  [ -n "${CTID:-}" ] || die "set CTID to the container to update"
  pct status "$CTID" >/dev/null 2>&1 || die "no container $CTID"
  say "Updating Flightplan in container $CTID"
  # keep its port and tunnel setting
  env_line="$(pct exec "$CTID" -- systemctl show flightplan.service -p Environment --value 2>/dev/null || true)"
  PORT="$(echo "$env_line" | tr ' ' '\n' | sed -n 's/^PORT=//p')"
  PORT="${PORT:-1826}"
  TUNNEL_IP="${TUNNEL_IP:-$(echo "$env_line" | tr ' ' '\n' | sed -n 's/^FP_TRUSTED_PROXIES=//p')}"
  install_into "$CTID"
  exit 0
fi

CTID="${CTID:-$(pvesh get /cluster/nextid)}"
pct status "$CTID" >/dev/null 2>&1 && die "container $CTID already exists (set CTID=...)"

if [ -z "${STORAGE:-}" ]; then
  if pvesm status -content rootdir 2>/dev/null | awk 'NR>1{print $1}' | grep -qx local-lvm; then
    STORAGE=local-lvm
  else
    STORAGE="$(pvesm status -content rootdir 2>/dev/null | awk 'NR>1 && $3=="active"{print $1; exit}')"
  fi
fi
[ -n "$STORAGE" ] || die "no storage for containers found (set STORAGE=...)"

net="name=eth0,bridge=$BRIDGE,ip=$IP"
[ "$IP" != dhcp ] && [ -n "${GW:-}" ] && net="$net,gw=$GW"
[ -n "${VLAN:-}" ] && net="$net,tag=$VLAN"

cat <<EOF

Flightplan container
  ID $CTID · $CT_HOSTNAME · $CORES cores · ${RAM} MB RAM + ${SWAP} MB swap · ${DISK} GB on $STORAGE
  network $BRIDGE, $IP${VLAN:+, VLAN $VLAN} · Flightplan on port $PORT
  ${TUNNEL_IP:+trusting forwarded headers only from $TUNNEL_IP (the Cloudflare tunnel)}
EOF
if [ "${YES:-}" != 1 ] && [ -t 0 ]; then
  read -r -p "Create it? [Y/n] " ok
  case "$ok" in [nN]*) exit 0 ;; esac
fi

say "Debian 12 template"
pveam update >/dev/null
TEMPLATE="$(pveam available --section system | awk '{print $2}' | grep -E '^debian-12-standard_.*_amd64\.tar\.(zst|gz|xz)$' | sort -V | tail -1)"
[ -n "$TEMPLATE" ] || die "no Debian 12 template offered by pveam"
if ! pveam list "$TEMPLATE_STORAGE" | grep -q "$TEMPLATE"; then
  pveam download "$TEMPLATE_STORAGE" "$TEMPLATE" >/dev/null
fi

say "Creating container $CTID"
pct create "$CTID" "$TEMPLATE_STORAGE:vztmpl/$TEMPLATE" \
  --hostname "$CT_HOSTNAME" --cores "$CORES" --memory "$RAM" --swap "$SWAP" \
  --rootfs "$STORAGE:$DISK" --net0 "$net" \
  --unprivileged 1 --features nesting=1 --onboot 1 --ostype debian \
  --tags flightplan --description "Flightplan flight planner (port $PORT). Update inside: flightplan-update" >/dev/null
pct start "$CTID"

say "Waiting for the network"
ctip=""
for _ in $(seq 1 60); do
  ctip="$(pct exec "$CTID" -- hostname -I 2>/dev/null | awk '{print $1}')" || true
  [ -n "$ctip" ] && pct exec "$CTID" -- getent hosts deb.debian.org >/dev/null 2>&1 && break
  sleep 2
done
[ -n "$ctip" ] || die "container $CTID has no network address"

say "Installing Flightplan in $CTID ($ctip)"
install_into "$CTID"

code="$(pct exec "$CTID" -- cat /opt/flightplan/data/setup-code.txt 2>/dev/null || true)"
cat <<EOF

Flightplan is running in container $CTID.
  Open:        http://$ctip:$PORT
  Setup code:  ${code:-see: pct exec $CTID -- cat /opt/flightplan/data/setup-code.txt}
  Chart data:  building now (10 to 20 min): pct exec $CTID -- journalctl -fu flightplan-data
  Tunnel:      in Cloudflare Zero Trust, point a public hostname at http://$ctip:$PORT
               (cloudflared can run on any machine on this network)
  Update:      copy a newer package here, unpack it, then: CTID=$CTID UPDATE=1 bash flightplan/deploy/flightplan-lxc.sh
EOF
