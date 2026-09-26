# Liholiswano v2

Liholiswano is a rotating savings group protocol built for Stellar/Soroban.

## Current Testnet pilot

- Network: Stellar Testnet
- Contract: `CAGSH4W3EYKOBHV6TUZ2WMKHKMZP6NNID5PLERFEV2EG6TRZWBZRKLY`
- Web app: `index.html`
- Testnet app deployment: GitHub Pages workflow in `.github/workflows/pages.yml`

The contract has been deployed and exercised on Stellar Testnet with real Testnet transactions covering token transfers, multiple rounds, expected invalid-bid rejection, and the final refund path.

## What the web app currently supports

- Create or import a Testnet wallet in browser memory
- Test XLM funding through Friendbot
- Configure the deployed contract and group token
- Read the protocol token allowlist
- Approve a token when using the protocol-admin wallet
- Enumerate groups
- Load a complete group dashboard
- Create and lock a group
- Join a group
- Join and promote from the FIFO waitlist
- Contribute
- Submit compulsory bids
- Settle rounds
- Mark a member defaulted (admin threshold 1 in the current browser UI)
- Claim completion refunds
- Export group state as JSON
- Open contract and transaction records in Stellar Expert

## Important pilot limitations

This is **Testnet software, not a production financial service**.

1. The browser pilot uses a Testnet secret key in memory. A production release should use a wallet connection such as Freighter and should never ask users to paste secret keys.
2. The current browser UI signs with one key, so group creation/locking/default actions are exposed with threshold 1. The Soroban contract itself supports N-of-M multi-admin authorization.
3. Token amounts are currently interpreted as 7-decimal units, matching the current Testnet token used by the pilot. A production token configuration must read and enforce the correct asset decimals.
4. The app does not yet provide fiat/mobile-money cash-in or cash-out for BWP/SZL.
5. KYC/AML, legal classification, production custody/payment arrangements, monitoring, security audit, and Mainnet configuration remain required before real-money use.
6. Never reuse Testnet token or issuer configuration on Mainnet without checking the authoritative issuer documentation.

## Development direction

The financial state machine is in the Soroban contract. The browser app is a Testnet integration layer. The next major engineering work is wallet integration, production-grade backend/indexing/notifications, fiat/payment rails, security review, compliance, and controlled pilot operations.
