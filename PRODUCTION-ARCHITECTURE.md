# Liholiswano Financial Services Platform — Production Architecture

Layers:
1. Client: registration, login, dashboard, savings groups, wallet, transactions, notifications and support.
2. API: authentication, authorization, KYC state, group orchestration, transaction ledger, notifications and audit.
3. PostgreSQL: users, wallets, groups, memberships, transactions, notifications and audit records.
4. KYC provider: external identity verification with signed webhooks as the source of truth.
5. Stellar/Soroban: settlement and group state; protocol-admin secrets stay server-side.
6. Operations: admin/compliance roles, monitoring, backups, incident response and key rotation.
7. Compliance: country-specific licensing, AML/CFT, consumer protection, privacy and retention review before Mainnet.

Security: bcrypt password hashing, short-lived JWT sessions, Helmet, CORS controls, rate limiting, server-side KYC state and audit logging.

Release gates: automated tests, dependency audit, contract audit, backend integration tests, KYC verification, payment reconciliation tests, backup/restore, monitoring, regulatory sign-off and staged Mainnet rollout.