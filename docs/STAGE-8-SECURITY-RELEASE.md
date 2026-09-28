# Stage 8 — Security, QA and Production Release Engineering

Status: ENGINEERING COMPLETE — final external verification remains

## Backend controls
- Helmet security middleware.
- Express x-powered-by disabled.
- General, authentication and verification-code rate limits.
- JWT expiry of 30 minutes.
- bcrypt password hashing.
- Zod request validation.
- Email verification gate.
- KYC verification gate for financial operations.
- Role-based authorization.
- Protected reconciliation endpoint.
- Duplicate Stellar transaction protection.
- Server-side Stellar transaction verification.
- No treasury private key stored by the API.

## Frontend controls
- No private keys or provider secrets.
- Wallet signing is performed by the user's wallet.
- Mainnet configuration separated from active Testnet configuration.
- Static security headers configured on Render.
- Frontend is not trusted to declare a payment successful.

## Data controls
- PostgreSQL constraints and indexes.
- Audit log.
- KYC event history.
- Verification-token expiry and one-time use.
- Password-reset token expiry and one-time use.
- Unique Stellar transaction hash.
- Funding-order idempotency key.

## Release gates
Before Mainnet public activation, verify:
1. Mainnet contract deployment and initialization.
2. Mainnet USDC allowlist on-chain.
3. Mainnet Wasm fingerprint recorded.
4. Production KYC provider acceptance test.
5. Production email delivery test.
6. Production SMS delivery test.
7. Mainnet API health.
8. Database migrations.
9. Registration, login and email verification.
10. KYC session and callback.
11. Wallet linking.
12. Contract read and controlled write.
13. Operation-verified funding confirmation.
14. Reconciliation.
15. Owner/compliance permissions.
16. Database backup/recovery arrangements.
17. Botswana and Eswatini privacy, AML/KYC and other applicable regulatory review.

## Rollback principle
If an application deployment fails, return the API/frontend to the last known-good application version while preserving on-chain state. Do not attempt to roll back the blockchain contract by simply changing the frontend.

External provider activation, production credentials, Mainnet deployment, operational testing and legal/regulatory sign-off remain separate gates.
