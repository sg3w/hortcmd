# WebDAV over HTTP and HTTPS for the harness (TICKET-038).
#
# Apache with mod_dav: the reference implementation, so a failure here is
# hortcmd's and not the server's.
#
# Debian rather than Alpine: mod_dav_fs keeps its lock database through
# an apr-util DBM driver, and Alpine ships no DBM backend for apr-util at
# all — every MKCOL then fails with "Could not open the lock database".
ARG BASE
FROM ${BASE}

RUN apt-get update \
 && apt-get install -y --no-install-recommends apache2 apache2-utils ssl-cert \
 && rm -rf /var/lib/apt/lists/*

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
