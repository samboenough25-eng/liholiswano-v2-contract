# Liholiswano Production / Mainnet Readiness

Updated: 2026-09-28

## Purpose

This document is the release gate for moving Liholiswano from the verified Stellar Testnet environment to a real-money Mainnet environment.

**Important:** Testnet success does not mean Mainnet is activated.

## Current verified state

- Stellar contract: deployed on Testnet.
- Existing-contract Testnet live cycle: passed.
- Testnet cycle covered group creation, join, lock, three rounds, settlement flow, and end-of-cycle refunds.
- Refund test completed with 3 of 3 refunds claimed and 0 test tokens remaining in the contract.
- Render API: live.
- Render PostgreSQL: available.
- Frontend Render static site exists.
- Internal reconciliation scheduler: enabled every 15 minutes on the single API instance.
- Mainnet readiness workflow exists and is intentionally non-deploying.

## Release gates

### 1. Smart contract
- [x] Contract builds in CI.
- [x] Contract tests pass in CI.
- [x] Existing deployed Testnet contract passes live acceptance cycle.
- [ ] Mainnet WASM build passes the final release candidate.
- [ ] Final contract hash is recorded and reviewed.
- [ ] Mainnet contract is deployed.
- [ ] Mainnet initialization is completed with the intended administrator.
- [ ] Mainnet contract read/invoke smoke tests pass.

### 2. Mainnet Stellar configuration
- [ ] Mainnet network selected only after all other release gates pass.
- [ ] Mainnet USDC issuer verified from an authoritative current source.
- [ ] Mainnet USDC Stellar Asset Contract address resolved for that issuer.
- [ ] Any other supported asset issuer/contract address verified.
- [ ] Mainnet treasury public key created and independently verified.
- [ ] Mainnet treasury signing process tested.
- [ ] No Testnet asset issuer, RPC URL, passphrase, contract ID, or treasury address remains in production configuration.

### 3. Backend
- [x] Production JWT secret is required.
- [x] Database connection is required.
- [x] Authentication and rate limits are implemented.
- [x] Email verification is enforced before financial operations.
- [x] KYC verification is enforced before wallet/group/funding operations where required.
- [x] Funding confirmation verifies the Stellar transaction and matching payment operation.
- [x] API does not hold the treasury private key.
- [ ] Production email provider connected and delivery tested.
- [ ] Production SMS provider connected and delivery tested.
- [ ] Real KYC provider connected and sandbox verification tested.
- [ ] Mainnet environment variables configured.
- [ ] Mainnet API smoke test completed.
- [ ] Recovery/reset flows tested with production providers.

### 4. KYC / compliance
- [x] KYC session/status data model exists.
- [x] KYC enforcement exists.
- [x] Manual compliance review route exists.
- [ ] Approved KYC provider selected.
- [ ] Provider account created.
- [ ] Provider credentials stored securely in Render, not source control.
- [ ] Document verification tested.
- [ ] Face/liveness testing completed if required by provider.
- [ ] AML/PEP screening integration tested if required.
- [ ] Webhook/result verification implemented and tested.
- [ ] Botswana and Eswatini legal/compliance requirements reviewed by qualified local advisers before public real-money launch.

### 5. Frontend
- [ ] Frontend production configuration points to the intended Mainnet API.
- [ ] Mainnet contract ID is correct.
- [ ] Mainnet network is shown clearly to users.
- [ ] Wallet connection/linking is tested.
- [ ] Registration/email verification is tested.
- [ ] KYC journey is tested.
- [ ] Group lifecycle is tested from the real UI.
- [ ] Funding and transaction history are tested.
- [ ] Owner/admin/compliance portals are tested.
- [ ] No Testnet faucet, test token, test issuer, or Testnet instructions are exposed in production UI.

### 6. Operations and security
- [ ] Render API health check is confirmed active on /health.
- [ ] Database backup/recovery procedure tested.
- [ ] Monitoring and alerting configured.
- [ ] Incident/contact procedure documented.
- [ ] Admin credentials protected with strong unique credentials.
- [ ] Treasury signing procedure uses a controlled wallet.
- [ ] No private keys are committed to GitHub.
- [ ] Dependency/security review completed.
- [ ] Production load/rate-limit smoke test completed.
- [ ] Reconciliation has a single active scheduler or a database lock if the API is later scaled beyond one instance.

### 7. Final release sequence

1. Freeze the release candidate.
2. Run the Mainnet readiness workflow.
3. Independently verify Mainnet asset addresses.
4. Configure production providers and secrets.
5. Deploy the Mainnet contract.
6. Initialize and verify the contract.
7. Configure backend for Mainnet.
8. Deploy backend and verify /health and production smoke tests.
9. Configure frontend for Mainnet.
10. Run a controlled end-to-end transaction with a small operational amount.
11. Review logs, reconciliation, database records, and on-chain records.
12. Only then open the service for normal customer use.

## Current blockers

The following are still release blockers:

1. Real KYC provider is not connected; the application currently uses `KYC_PROVIDER=stub`.
2. Production email provider is not configured.
3. Production SMS provider is not configured.
4. Mainnet treasury and asset configuration are not verified.
5. Mainnet contract has not been deployed.
6. Backend is currently configured for Stellar Testnet.
7. Frontend production Mainnet activation has not been verified.
8. Legal/compliance sign-off for the actual operating model is not documented.

## Render health-check note

The repository Blueprint defines `healthCheckPath: /health`, and the API implements `GET /health` with a database connectivity check. Render supports HTTP health checks through the Blueprint `healthCheckPath` field. The live Render service metadata previously showed an empty health-check setting, so the Dashboard/service configuration should be explicitly verified before production.

## Safety rule for this repository

Do not replace Testnet values with Mainnet values merely to make the application appear production-ready. Mainnet activation is a separate release stage and must use independently verified Mainnet addresses and production provider credentials.
