#!/bin/sh
# HARNESS_TLS=0 serves plain HTTP on 80, =1 serves HTTPS on 443 with the
# run's generated certificate.
#
# Debian's apache2 is configured the Debian way — stock apache2.conf,
# a2enmod, one site — rather than by writing a config from scratch. Half
# the modules are built into this build and refuse to be loaded again,
# and which half is not something a harness should have to track.
set -eu

: "${HARNESS_USER:?}" "${HARNESS_PASSWORD:?}" "${HARNESS_TLS:?}"

mkdir -p /data
chown -R www-data /data

htpasswd -bc /etc/apache2/dav.passwd "$HARNESS_USER" "$HARNESS_PASSWORD" >/dev/null 2>&1

# Logs belong in the container log, not in a file nobody will read.
ln -sf /dev/stdout /var/log/apache2/access.log
ln -sf /dev/stderr /var/log/apache2/error.log
ln -sf /dev/stderr /var/log/apache2/other_vhosts_access.log

a2enmod -q dav dav_fs auth_basic authn_file authz_user headers
echo "ServerName localhost" > /etc/apache2/conf-available/servername.conf
a2enconf -q servername
a2dissite -q 000-default

if [ "$HARNESS_TLS" = "1" ]; then
  a2enmod -q ssl
  PORT=443
  TLS_LINES="SSLEngine on
  SSLCertificateFile /run/secrets/server.crt
  SSLCertificateKeyFile /run/secrets/server.key
  SSLProtocol -all +TLSv1.2 +TLSv1.3"
else
  PORT=80
  TLS_LINES=""
fi

echo "Listen ${PORT}" > /etc/apache2/ports.conf

cat > /etc/apache2/sites-available/harness.conf <<CONF
<VirtualHost *:${PORT}>
  DocumentRoot /data
  ${TLS_LINES}
  <Directory /data>
    Dav On
    Options Indexes
    AllowOverride None
    AuthType Basic
    AuthName "hortcmd harness"
    AuthUserFile /etc/apache2/dav.passwd
    Require valid-user
  </Directory>
</VirtualHost>
CONF
a2ensite -q harness

exec apachectl -DFOREGROUND
