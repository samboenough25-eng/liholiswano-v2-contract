# Stage 7 — Frontend Mainnet Engineering

Status: ENGINEERING COMPLETE — activation remains gated

## Goal
Keep the live frontend safely on Testnet while preparing a controlled Mainnet configuration.

## Verified Mainnet public values
- Network passphrase: Public Global Stellar Network ; September 2015
- Horizon: https://horizon.stellar.org
- USDC issuer: GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN
- USDC SAC: CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75

The real Mainnet contract ID is deliberately not hard-coded until Stage 5 produces it.

## Frontend architecture
The active network configuration must contain network, passphrase, Soroban RPC, Horizon URL, contract ID, USDC issuer/SAC and API URL.

No wallet secret, API secret, Smile ID secret, Resend key, SMS key or Render secret belongs in the frontend.

## Current protection
- web/testnet-config.js remains active.
- web/mainnet-config.template.js is template-only.
- The Mainnet template is not loaded by the live site.
- Current customer and owner payment helpers explicitly use Testnet.

## Activation sequence
1. Insert the real Stage 5 Mainnet contract ID.
2. Set the Mainnet API URL only after Stage 6 passes.
3. Make wallet, contract and asset operations read the active network configuration instead of hard-coded Testnet endpoints.
4. Run the frontend release checks.
5. Deploy the static site.
6. Verify login, KYC, wallet connection, contract reads and a controlled transaction.
7. Never expose a Mainnet signing secret.

The backend remains the authority for financial confirmation.
