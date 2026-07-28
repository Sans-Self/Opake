#!/usr/bin/env bash
# Create (or recreate) the dev-env CA and leaf cert from scratch.
#
# These are throwaway TLS credentials for the hermetic local stack; they are
# committed to the repo on purpose so the stack works out of the box. Never
# add this CA to a real trust store, and never reuse these keys outside
# dev-env. Rerun after editing san.ext (see dev-env/README.md on adding a
# PDS vhost).
set -euo pipefail
cd "$(dirname "$0")"

DAYS=3650

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout ca.key -out ca.crt -days "$DAYS" \
  -subj "/CN=Opake dev-env CA"

openssl req -newkey rsa:2048 -nodes \
  -keyout pds.key -out pds.csr \
  -subj "/CN=pds-a.test"

openssl x509 -req -in pds.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -out pds.crt -days "$DAYS" -extfile san.ext

rm -f pds.csr ca.srl

echo "Regenerated dev CA and leaf cert (valid ${DAYS}d)."
echo "Rebuild/restart the dev-env stack so containers pick up the new CA."
