#!/bin/sh
# Writes a ProFTPD config for the requested TLS mode and runs it in the
# foreground.
#
# HARNESS_FTP_MODE: plain | explicit | implicit
#   plain     — no TLS at all; the case that leaks credentials, kept so
#               the harness can prove hortcmd still talks to it.
#   explicit  — AUTH TLS on the normal port.
#   implicit  — TLS from the first byte (classically port 990).
set -eu

: "${HARNESS_USER:?}" "${HARNESS_PASSWORD:?}" "${HARNESS_FTP_MODE:?}"
: "${HARNESS_PASV_MIN:?}" "${HARNESS_PASV_MAX:?}"

if ! id "$HARNESS_USER" >/dev/null 2>&1; then
  useradd -d /data -s /bin/sh "$HARNESS_USER"
  echo "$HARNESS_USER:$HARNESS_PASSWORD" | chpasswd
fi
mkdir -p /data /var/run/proftpd
chown -R "$HARNESS_USER" /data

cat > /etc/proftpd/proftpd.conf <<CONF
ServerName "hortcmd harness"
ServerType standalone
DefaultServer on
UseIPv6 off
Port 21
User proftpd
Group nogroup
Umask 022
# Confined to /data, which is how an FTP account is normally deployed —
# and what makes the traversal check meaningful: paths are then relative
# to the tree, and a ".." has somewhere to try to escape to.
DefaultRoot /data
RequireValidShell off
AllowOverwrite on
TimesGMT off
# ProFTPD refuses to open a symlinked log path, which /dev/stderr is —
# so it writes real files and tails carry them to the container log.
SystemLog /var/log/proftpd.log
TransferLog /var/log/xfer.log
WtmpLog off

# Passive mode hands the client a port; the range must match what the
# compose file publishes, one for one.
PassivePorts ${HARNESS_PASV_MIN} ${HARNESS_PASV_MAX}
MasqueradeAddress 127.0.0.1

<Limit LOGIN>
  AllowUser ${HARNESS_USER}
  DenyAll
</Limit>
CONF

if [ "$HARNESS_FTP_MODE" != "plain" ]; then
  cat >> /etc/proftpd/proftpd.conf <<CONF

LoadModule mod_tls.c
<IfModule mod_tls.c>
  TLSEngine on
  TLSLog /var/log/proftpd.log
  TLSProtocol TLSv1.2 TLSv1.3
  TLSRSACertificateFile /run/secrets/server.crt
  TLSRSACertificateKeyFile /run/secrets/server.key
  # Control *and* data must be encrypted: a client that negotiates TLS
  # for the login and then sends the file in the clear is the failure
  # this harness exists to notice.
  TLSRequired on
  TLSVerifyClient off
  # Session reuse on the data connection is a compatibility minefield
  # and orthogonal to what is being tested here.
  TLSOptions NoSessionReuseRequired
CONF
  if [ "$HARNESS_FTP_MODE" = "implicit" ]; then
    echo "  TLSOptions NoSessionReuseRequired UseImplicitSSL" \
      >> /etc/proftpd/proftpd.conf
  fi
  echo "</IfModule>" >> /etc/proftpd/proftpd.conf
fi

for log in /var/log/proftpd.log /var/log/xfer.log; do
  : > "$log"
  tail -F "$log" &
done

exec proftpd --nodaemon --config /etc/proftpd/proftpd.conf
