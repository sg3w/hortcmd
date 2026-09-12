# SMB2/3 for the harness (TICKET-038).
#
# Samba with signing *mandatory*: unsigned SMB is trivially tampered with,
# and a client that quietly negotiates it away would never be noticed
# without a server that refuses.
ARG BASE
FROM ${BASE}

RUN apk add --no-cache samba samba-common-tools

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

EXPOSE 445
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
