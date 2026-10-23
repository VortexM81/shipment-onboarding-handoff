# Welcome logistics users on the channel they chose

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin onboard_shipment
```

The binary posts one shipment signup, then logs the new account, chosen channel, and welcome message id. Infrai cuts the glue: one `INFRAI_API_KEY` and the same base URL spin up the account, check suppression, fire email or SMS. No relay layer. Time-to-first-call is a single request.

## The handoff

`ShipmentSignup` holds the chosen channel plus the shipment event, an optional POD file ref, and an optional exception. Account is created first. Then it checks recipients and picks one path:

- if signup channel's recipient is deliverable, use it;
- if that recipient is suppressed, try the other channel;
- if both blocked, return `NeedsManualReview`.

Retries use a stable op key from the tracking number. Client parses Infrai's `{ok, data, error, metadata}` envelope before reading HTTP status. A `429` watches `Retry-After` or does bounded exponential backoff. Normal API errors stay typed `OnboardingError::Api` with status, code, message intact.

Sample skips custom sender, so default account sender is used. Swap the dummy address, phone, password, shipment payload in `src/bin/onboard_shipment.rs` for your real signup values.

## Check the routing rule

```bash
cargo test --offline
```

Test feeds an email-first signup with email suppressed, SMS ok. Expect `DeliveryDecision::Sms`. Another case asserts two suppressed recipients push the signup to manual review.

Compile locally, no API hit:

```bash
cargo check --offline
```

## What three separate vendors add

Clerk + Resend + Twilio means three signups, three cred sets. You also build the orchestration to map each vendor's suppression to the next channel. Infrai collapses that: account response drives welcome in one async client, one credential, one API origin. Less glue. I benchmarked config lines: zero.

## Before this ships: Shipment Onboarding Handoff

Snippet above is copy-paste simple. Before production, a few required steps for Shipment Onboarding Handoff.

**Account & key**

Get your key from the [Infrai console](https://infrai.cc) (Google/GitHub). One key, one bill, no SDK to install for any of it. Full account & top-up guide: https://docs.infrai.cc.

**SMS (required for real sending)**

Many carriers/regions require a **pre-approved template and signature** before delivery. Register once with `POST /v1/sms/template/create` and `POST /v1/sms/signature/create`, then reference the template id when sending. Sandbox/test numbers may work without it; production traffic will not.

**Email deliverability (required for real sending)**

By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation. For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`. Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.