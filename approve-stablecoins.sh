#!/usr/bin/env bash
set -euo pipefail
CONTRACT_ID="${CONTRACT_ID:?Set CONTRACT_ID to your deployed contract ID}"
NETWORK="${NETWORK:-testnet}"
PROTOCOL_ADMIN="${PROTOCOL_ADMIN:-admin}"
USDC_MAINNET_SAC="CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
USDT0_MAINNET_SAC="CBSJZEIO5C7KC2SF3MKSNXXJSW5G3VTNBX4ATMKUI3B2MR4JKM4R26YF"
USDC_TESTNET_SAC="CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA"
approve() {
  local label="$1" sac="$2"
  echo "Approving ${label} (${sac}) on ${NETWORK}..."
  stellar contract invoke \
    --id "$CONTRACT_ID" \
    --source-account "$PROTOCOL_ADMIN" \
    --network "$NETWORK" \
    -- add_approved_token \
    --protocol_admin "$PROTOCOL_ADMIN" \
    --token "$sac"
}
if [ "$NETWORK" = "testnet" ]; then
  approve "USDC (testnet)" "$USDC_TESTNET_SAC"
  echo "Skipping USDT0 — no Testnet deployment exists yet."
elif [ "$NETWORK" = "mainnet" ]; then
  approve "USDC (mainnet)" "$USDC_MAINNET_SAC"
  approve "USDT0 (mainnet)" "$USDT0_MAINNET_SAC"
else
  echo "Unknown NETWORK: $NETWORK" >&2
  exit 1
fi
echo "Done. Verify with: stellar contract invoke --id \"$CONTRACT_ID\" --source-account \"$PROTOCOL_ADMIN\" --network \"$NETWORK\" -- list_approved_tokens"
