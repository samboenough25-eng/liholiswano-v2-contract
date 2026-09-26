# Liholiswano Production API

This is the application/backend layer around the Soroban contract. It provides registration, login, roles, KYC state, wallet linking, transaction history, notifications and audit logging.

Setup:
1. Provision PostgreSQL.
2. Copy .env.example to .env and set real secrets.
3. Run the schema in src/schema.sql against the database.
4. Run npm install and npm start.

Production provider activation is still required for KYC, email/SMS/push and any custodial payment operations. Never put Stellar secret keys or identity documents in the browser.