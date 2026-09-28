# Stage 4 — Mainnet Stellar Preparation

Updated: 2026-09-28

## Objective

Prepare every Stellar Mainnet value and control needed for the later Mainnet contract deployment and production activation, without changing the live Testnet service.

## Verified Mainnet network values

- Network: Mainnet / Public Global Stellar Network
- Network passphrase: `Public Global Stellar Network ; September 2015`
- Mainnet network ID: `7ac33997544e3175d266bd022439b22cdb16508c01163f26e5cb2a3e1045a979`
- Public Horizon: `https://horizon.stellar.org`

Stellar documents these Mainnet values and states that Mainnet and Testnet use different passphrases and network IDs. A transaction signed for one network is not valid for the other.

## Mainnet RPC

Stellar does not provide an SDF-hosted public Mainnet RPC endpoint. A production application therefore needs an ecosystem RPC provider or its own RPC infrastructure.

Current Stellar documentation lists Mainnet providers including Gateway, sorobanrpc.com, Nodies, OnFinality, Lightsail Network and Ankr.

The repository intentionally does NOT select a provider by itself. The production provider must be selected based on reliability, rate limits, SLA, geographic/network requirements and cost.

## USDC — independently verified

The current official/authoritative Stellar documentation identifies Circle's native Stellar USDC as:

- Asset code: `USDC`
- Mainnet issuer: `GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`
- Mainnet Stellar Asset Contract / SEP-41 contract: `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75`

Circle's current USDC-on-Stellar page confirms a separate Mainnet address from the Testnet address. Stellar's current developer documentation also lists the Mainnet issuer and SAC address.

Do not substitute another asset merely because its code is `USDC`. Stellar can contain multiple assets with the same code but different issuers.

## Existing Testnet values — DO NOT CHANGE YET

Current verified Testnet contract:

`CBSBXWQFE3SOT3BOPVZ2IHK32PBUP5BFRHJLI6R2WH6WIQJQLV4WAYHO`

Current Testnet USDC issuer:

`GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5`

Current Testnet USDC SAC:

`CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA`

These remain the active application values until the later Mainnet activation stages.

## Mainnet accounts that must be created/selected

### Protocol administrator

A dedicated Mainnet administrator public key must be selected.

Requirements:

- controlled by the project
- not stored with the backend application
- secret key held in secure signing storage
- public key independently recorded
- used only for contract administration
- recovery procedure documented

### Treasury

A dedicated Mainnet treasury public key must be selected.

Requirements:

- separate from normal customer wallets
- controlled by a secure signing process
- sufficient XLM for network reserves/fees
- sufficient supported-asset balance for actual funding operations
- public key recorded in the database only
- private key never stored in GitHub or Render application variables

The backend architecture already avoids holding the treasury private key.

## Mainnet contract

Stage 4 does NOT deploy the contract.

Stage 5 will:

1. build the final release candidate;
2. record the exact WASM SHA-256;
3. deploy the WASM to Mainnet;
4. record the resulting Mainnet contract ID;
5. initialize it with the approved administrator;
6. configure the intended supported asset;
7. run read/invoke smoke tests;
8. independently verify the resulting state.

The Testnet contract ID must never be reused as the Mainnet contract ID.

## Mainnet environment template

A separate template was added:

`web/mainnet-config.template.js`

It is deliberately not referenced by the live Testnet frontend.

It contains placeholders for:

- Mainnet RPC provider
- Mainnet contract ID
- treasury public key
- protocol-admin public key

It contains the verified Mainnet network and USDC values.

## Production backend values — later Stage 6

When Stage 6 is reached, the API will use values equivalent to:

`STELLAR_NETWORK=mainnet`

`STELLAR_RPC_URL=<selected production Mainnet RPC>`

`STELLAR_CONTRACT_ID=<Stage-5 Mainnet contract ID>`

The Testnet values must remain untouched until the production cutover is deliberately performed.

## Release controls

Before Stage 5:

- [x] Mainnet passphrase verified.
- [x] Mainnet network ID verified.
- [x] Mainnet Horizon endpoint identified.
- [x] Mainnet USDC issuer verified.
- [x] Mainnet USDC SAC verified.
- [x] Testnet values isolated from Mainnet template.
- [x] Mainnet readiness workflow exists.
- [ ] Production Mainnet RPC provider selected.
- [ ] Mainnet protocol-admin public key selected and independently verified.
- [ ] Mainnet treasury public key selected and independently verified.
- [ ] Mainnet treasury signing/recovery procedure tested.
- [ ] Final Mainnet WASM build/hash approved.
- [ ] Mainnet contract deployed and initialized.

## Safety rule

No Mainnet transaction should be signed merely because the configuration file exists.

Stage 4 prepares and verifies configuration. Stage 5 is the controlled Mainnet contract deployment. Stage 6 changes the production backend. Stage 7 activates the frontend.

