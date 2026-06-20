#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"

echo "==> GET /health"
curl -fsS "${BASE_URL}/health" | tee /tmp/platform-health.json
echo

echo "==> POST /user/request-signature"
curl -fsS -X POST "${BASE_URL}/user/request-signature" \
  -H 'Content-Type: application/json' \
  -d '{"publicKey":"0x0000000000000000000000000000000000000001"}' \
  | tee /tmp/platform-signature.json
echo

echo "Smoke test passed against ${BASE_URL}"
