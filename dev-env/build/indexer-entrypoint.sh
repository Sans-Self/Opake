#!/bin/sh
# Dev-env entrypoint wrapper: trust the local dev CA before starting the
# indexer, so its Req/Mint client (OTP :public_key.cacerts_get, i.e. the OS
# trust store) can fetch at.opake.publicKey records + backfill over the PDSes'
# https://pds-x.test endpoints. Config-only: the app's own entrypoint/source is
# untouched; this just augments the OS trust store at boot.
set -e
if [ -f /certs/ca.crt ]; then
  cp /certs/ca.crt /usr/local/share/ca-certificates/opake-devca.crt
  /usr/sbin/update-ca-certificates >/dev/null 2>&1 || true
fi
exec /app/entrypoint.sh
