# Stage 3 — Production Email and SMS

## Providers
- Transactional email: Resend
- SMS/phone verification: Africa's Talking

## Implemented
- Resend email verification is integrated.
- Resend password-reset email is integrated.
- Production startup requires email provider configuration before Mainnet.
- Africa's Talking SMS support is integrated.
- Phone verification uses the common SMS adapter.
- SMS delivery-report endpoint: /api/sms/delivery-report.
- Provider selection is environment-controlled.
- Provider secrets are not stored in GitHub source.
- Testnet remains active.

## Render non-secret configuration
- EMAIL_PROVIDER=resend
- SMS_PROVIDER=africastalking
- AT_SMS_URL=https://api.africastalking.com/version1/messaging

## Secrets required
Enter these directly into Render Environment Variables. Never commit them.

Resend:
- RESEND_API_KEY
- EMAIL_FROM

Africa's Talking:
- AT_USERNAME
- AT_API_KEY
- AT_SENDER_ID

## Testing
### Email
1. Verify the sending domain in Resend.
2. Create a restricted sending API key.
3. Add RESEND_API_KEY and EMAIL_FROM to Render.
4. Register a test Liholiswano account.
5. Confirm the six-digit verification code arrives.
6. Test resend verification.
7. Test forgot-password email.
8. Check Resend logs.

### SMS
1. Create an Africa's Talking account/app.
2. Start in Sandbox.
3. Configure Sandbox credentials.
4. Configure an approved Sender ID for the intended production route.
5. Add AT_USERNAME, AT_API_KEY and AT_SENDER_ID to Render.
6. Register a phone number through Liholiswano.
7. Confirm the six-digit SMS arrives.
8. Verify the phone.
9. Test incorrect/expired codes.
10. Test rate limiting.
11. Move to live only after Sandbox passes.

## Production safety
Do not switch to Mainnet merely because email or SMS works. Mainnet still requires the Stellar, treasury, KYC, compliance, frontend and operational gates documented in docs/MAINNET-READINESS.md.

Africa's Talking distinguishes immediate API acceptance from final handset delivery. Liholiswano therefore treats an immediate SMS response as accepted, not proof of delivery.

## Security
- API keys stay in Render secrets.
- Never put provider keys in frontend JavaScript.
- Never print provider keys in logs.
- Use minimum Resend API-key permission needed for sending.
- Rotate provider keys if exposed.