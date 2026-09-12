#!/bin/sh
# Creates the run's user from the generated credentials, then runs sshd in
# the foreground so Docker owns the process lifetime.
set -eu

: "${HARNESS_USER:?HARNESS_USER is required}"
: "${HARNESS_PASSWORD:?HARNESS_PASSWORD is required}"

if ! id "$HARNESS_USER" >/dev/null 2>&1; then
  adduser -D -h "/home/$HARNESS_USER" "$HARNESS_USER"
  echo "$HARNESS_USER:$HARNESS_PASSWORD" | chpasswd
fi
mkdir -p "/home/$HARNESS_USER/.ssh"
chown -R "$HARNESS_USER" "/home/$HARNESS_USER"
chmod 700 "/home/$HARNESS_USER/.ssh"

# The mounted seed tree belongs to the run's user.
mkdir -p /data
chown -R "$HARNESS_USER" /data

# sshd refuses a group- or world-readable host key, and the mount is
# read-only — so the key is copied in rather than chmod'ed in place.
# Copying also means the harness can swap the mounted key and restart the
# container to present a *different* host key (the rotation case).
install -m 600 /run/secrets/ssh_host_ed25519_key /etc/ssh/ssh_host_ed25519_key

exec /usr/sbin/sshd -D -e
