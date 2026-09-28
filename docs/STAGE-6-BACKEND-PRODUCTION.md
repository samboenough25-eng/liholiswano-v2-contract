# Stage 6 — Backend Production / Mainnet Engineering

Status: ENGINEERING COMPLETE — activation remains gated

## Purpose
Stage 6 prepares the Render API for a real Mainnet configuration without switching the currently live service to Mainnet.

## Required Mainnet configuration
- NODE_ENV=production
- STELLAR_NETWORK=mainnet
- STELLAR_RPC_URL=<approved Mainnet Soroban RPC>
- STELLAR_CONTRACT_ID=<real deployed Mainnet contract C...>
- STELLAR_HORIZON_URL=https://horizon.stellar.org
- MAINNET_USDC_ISSUER=GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN
- MAINNET_USDC_SAC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75
- KYC_PROVIDER=smile
- SMILE_PARTNER_ID=<secret>
- SMILE_API_KEY=<secret>
- KYC_CALLBACK_URL=https://liholiswano-api.onrender.com/api/kyc/callback
- EMAIL_PROVIDER=resend
- RESEND_API_KEY=<secret>
- EMAIL_FROM=<verified sender>
- SMS_PROVIDER=africastalking
- AT_USERNAME=<account/config>
- AT_API_KEY=<secret>
- AT_SENDER_ID=<approved sender>
- RECONCILE_SECRET=<secret>

Never put private signing keys in the web application.

## Safety gates
The backend must refuse Mainnet startup when production KYC is still stub, communications are incomplete, or the Mainnet contract/asset configuration is missing.

## Cutover order
1. Complete and verify Stage 5 Mainnet contract deployment.
2. Record the Mainnet contract ID and Wasm fingerprint.
3. Configure Mainnet public values and provider credentials in Render.
4. Deploy the API.
5. Confirm /health reports network: mainnet.
6. Test registration, email verification, login, KYC session creation and phone verification.
7. Test a real Stellar read against the Mainnet contract.
8. Only then activate the Mainnet frontend.

Current live API remains on Testnet. This is intentional.
