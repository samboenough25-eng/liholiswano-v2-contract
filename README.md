# Liholiswano

Liholiswano is a Testnet-first rotating savings group platform for Botswana and Eswatini, with a Node/Express API, PostgreSQL, GitHub Pages portals, and a Soroban smart contract.

## Current deployment

- API: https://liholiswano-api.onrender.com
- Network: Stellar Testnet
- Database: Render PostgreSQL
- Web app: GitHub Pages workflow (.github/workflows/pages.yml)
- Contract ID: CAGSH4W3EYKOBHV6TUZ2WMKHKMZP6NNID5PLERFEV2EG6TRZWBZRKLY

## Implemented

- Customer, owner and compliance portals
- Password hashing, JWT sessions and role checks
- Email verification and password recovery interfaces
- Phone verification provider interface
- KYC session/status workflow and compliance review controls
- Stellar wallet linking
- Soroban group-management integration
- Blockchain transaction recording with duplicate protection
- Server-side Stellar transaction verification before a transaction is marked confirmed
- Reconciliation worker and protected reconciliation endpoint
- PostgreSQL schema bootstrap/migrations
- Automated JavaScript/backend checks

## External production dependencies still required

These cannot be safely invented or activated without the platform owner's provider accounts/configuration:

1. Resend (or another transactional email provider): RESEND_API_KEY and EMAIL_FROM.
2. SMS provider: SMS_PROVIDER_URL and SMS_PROVIDER_API_KEY.
3. Approved KYC provider or a documented human-review process: KYC_PROVIDER is currently stub by default.
4. A scheduler for /internal/reconcile, using RECONCILE_SECRET. The protected endpoint is implemented; scheduling requires a platform secret/configuration.
5. A production legal/compliance review before any Mainnet or real-money activation.

## Testnet safety

The application is configured around Stellar Testnet. Never put real wallet secret keys into the web application. Mainnet activation should be a separate controlled deployment after contract, asset, KYC, security and regulatory checks.