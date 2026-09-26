# Liholiswano KYC integration boundary

The current repository is a Testnet pilot. KYC is intentionally not implemented as a browser-only identity-verification system.

## What is implemented now
- Testnet wallet creation/import remains local to the browser session.
- The app can display a verification state and route users toward an external KYC service.
- No identity document, selfie, or credential is requested by the static pilot.

## What must be added before production
1. A regulated/approved KYC provider appropriate to the operating jurisdiction.
2. A backend API that creates verification sessions and receives provider webhooks.
3. Server-side verification-status storage keyed to an internal user ID.
4. Consent, privacy notice, retention/deletion controls, audit logging and access controls.
5. A policy preventing raw identity documents from being stored in the Stellar contract.
6. A rule that financial transactions requiring KYC are blocked until the backend reports a verified status.

## Important
Do not treat the Testnet pilot's UI status as proof of identity verification. Production KYC must be performed and attested by the selected provider/backend.
