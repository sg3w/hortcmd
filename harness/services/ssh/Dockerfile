# SFTP + SSH shell/PTY for the protocol harness (TICKET-038).
#
# One server serves both, which is the point: hortcmd's terminal rides on
# the very SSH session the SFTP provider verified (TICKET-028), so testing
# them against two different servers would test a shape the app never has.
#
# The base image is pinned by digest in harness/images.json — a harness
# that silently follows a moving tag is not a harness.
ARG BASE
FROM ${BASE}

RUN apk add --no-cache openssh openssh-sftp-server shadow

# The host key is generated per run and mounted, so a test can rotate it
# and watch the client refuse the changed key.
COPY sshd_config /etc/ssh/sshd_config
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

EXPOSE 22
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
