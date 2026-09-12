# FTP, explicit FTPS (AUTH TLS) and implicit FTPS for the harness
# (TICKET-038).
#
# One image, three roles: the mode is chosen by HARNESS_FTP_MODE so the
# three variants are the same server differing only in how TLS is applied
# — which is exactly the difference hortcmd's `Tls` enum encodes.
#
# ProFTPD rather than vsftpd, and Debian rather than Alpine, for two
# reasons found by running them: vsftpd on musl segfaults as soon as a
# TLS session is established, and vsftpd on glibc segfaults a little
# later, under the load this suite puts on it. A server that dies
# mid-suite turns every later assertion into a connection error.
# ProFTPD also supports implicit FTPS directly, which pure-ftpd does not.
ARG BASE
FROM ${BASE}

RUN apt-get update \
 && apt-get install -y --no-install-recommends proftpd-core proftpd-mod-crypto procps \
 && rm -rf /var/lib/apt/lists/*

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
