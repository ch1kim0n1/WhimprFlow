# Billing (one-time license)

WhimprFlow uses **Option A**: offline ed25519 license keys, sold as a one-time
purchase.

## Checkout

1. Create a product on Gumroad, Lemon Squeezy, or Paddle (VAT/tax handled by the provider).
2. Point `https://whimprflow.com/buy` (and the Hub Buy button) at that checkout URL.
3. After payment, deliver a license key (manual at first, or automate via webhook).

## Issue a key

```bash
# Private key: secrets/license-private.hex or WHIMPR_LICENSE_PRIVATE_KEY_HEX
cargo run -p whimpr-license -- issue --email buyer@example.com --tier pro
# Optional expiry:
cargo run -p whimpr-license -- issue --email buyer@example.com --tier pro --days 365
```

Email the printed `WF1....` string to the buyer. They activate it in **Hub > License**.

## Edge cases

| Case | Behavior |
| --- | --- |
| Expired card / no renewal | N/A for one-time; timed keys expire via `exp` |
| Refund / chargeback | Stop issuing keys; optionally rotate verify key (forces re-issue) |
| Machine transfer | Same key works offline on a new device (honor-system); revoke by rotating keys if abused |
| Lost key | Re-issue from purchase email after support verification |
| Trial | Hub > License > Start 14-day trial (keychain-backed start time) |

## In-app gate

Cloud cleanup (OpenAI / Anthropic) requires `Licensed` or `Trial`. Raw and local
cleanup remain available without payment.
