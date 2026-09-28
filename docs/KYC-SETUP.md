# Liholiswano — Stage 2 KYC Setup

## Provider

Liholiswano now supports Smile ID as the production KYC provider.

Smile ID lists both Botswana and Eswatini in its current country coverage and lists Botswana National Identity Card, Botswana Driver's License, Botswana Passport and Botswana Resident/Travel documents among supported document types. Smile ID also documents Hosted Web integration for Enhanced KYC and SmartSelfie, with an asynchronous callback to the partner backend.

## What has been implemented

- smile-identity-core server SDK added.
- /api/kyc/session can create a Smile ID Hosted Web KYC session.
- /api/kyc/callback verifies the Smile callback signature before changing a user's KYC state.
- Verified provider results update the Liholiswano user and KYC session.
- Rejected provider results update the user to rejected.
- The KYC audit trail records provider callbacks.
- Mainnet startup refuses to run with the stub provider.
- Testnet remains unchanged.

## Render environment variables

Non-secret configuration:

- KYC_PROVIDER=stub until the Smile account has been created and tested.
- SMILE_SERVER_URL=https://api.smileidentity.com
- SMILE_KYC_PRODUCT=enhanced_kyc
- KYC_CALLBACK_URL=https://liholiswano-api.onrender.com/api/kyc/callback

Secret configuration, which must be entered only in Render Environment Variables:

- SMILE_PARTNER_ID
- SMILE_API_KEY

Never put the API key in GitHub source code, HTML, JavaScript, chat messages, or this document.

## Activation sequence

1. Create/activate the Smile ID partner account and obtain Sandbox credentials.
2. Configure the Liholiswano callback URL in Smile ID.
3. Add SMILE_PARTNER_ID and SMILE_API_KEY to the Render API service.
4. Keep KYC_PROVIDER=smile for Sandbox testing only after the credentials are present.
5. Run an end-to-end test with a permitted test identity.
6. Confirm the callback is accepted and the Liholiswano user changes from in_progress to verified or rejected.
7. Only after Sandbox acceptance should production credentials be issued.
8. For production, use the production Smile endpoint and production credentials.
9. Complete the required privacy/data-processing and regulatory review for Botswana and Eswatini before real-money public launch.

## Important

A provider integration is not the same thing as regulatory approval. Liholiswano must still have the required legal/compliance basis for collecting and processing identity and biometric information in each country.

The platform must never mark a customer as KYC-verified merely because a session was created. Verification is granted only from a verified provider callback or an authorized manual review.
