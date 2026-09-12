#!/bin/sh
# One share, one user, signing required, SMB2 as the minimum dialect.
set -eu

: "${HARNESS_USER:?}" "${HARNESS_PASSWORD:?}" "${HARNESS_SHARE:?}"

if ! id "$HARNESS_USER" >/dev/null 2>&1; then
  adduser -D -H -s /sbin/nologin "$HARNESS_USER"
fi
printf '%s\n%s\n' "$HARNESS_PASSWORD" "$HARNESS_PASSWORD" \
  | smbpasswd -s -a "$HARNESS_USER"

mkdir -p /data /var/lib/samba/private /var/cache/samba
chown -R "$HARNESS_USER" /data

cat > /etc/samba/smb.conf <<CONF
[global]
  workgroup = WORKGROUP
  server string = hortcmd harness
  security = user
  map to guest = Never
  server min protocol = SMB2_10
  server max protocol = SMB3
  server signing = mandatory
  smb ports = 445
  log level = 2
  log file = /dev/stdout
  max log size = 0
  panic action = /bin/true
  disable netbios = yes
  smb1 unix extensions = no

[${HARNESS_SHARE}]
  path = /data
  browseable = yes
  writable = yes
  guest ok = no
  valid users = ${HARNESS_USER}
  create mask = 0644
  directory mask = 0755
CONF

testparm -s >/dev/null
exec smbd --foreground --no-process-group --debug-stdout
