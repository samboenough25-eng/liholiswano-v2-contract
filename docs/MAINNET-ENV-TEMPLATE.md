# Mainnet environment template

Public configuration names only. No secrets.

NODE_ENV=production
STELLAR_NETWORK=mainnet
STELLAR_RPC_URL=<approved-mainnet-soroban-rpc>
STELLAR_HORIZON_URL=https://horizon.stellar.org
STELLAR_CONTRACT_ID=<MAINNET_CONTRACT_ID_FROM_STAGE_5>
MAINNET_USDC_ISSUER=GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN
MAINNET_USDC_SAC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75
KYC_PROVIDER=smile
SMILE_PARTNER_ID=<secret>
SMILE_API_KEY=<secret>
KYC_CALLBACK_URL=https://liholiswano-api.onrender.com/api/kyc/callback
EMAIL_PROVIDER=resend
RESEND_API_KEY=<secret>
EMAIL_FROM=<verified-sender>
SMS_PROVIDER=africastalking
AT_USERNAME=<account-username>
AT_API_KEY=<secret>
AT_SENDER_ID=<approved-sender-id>
RECONCILE_SECRET=<secret>

Store secrets in Render secret environment variables. Never commit them.
