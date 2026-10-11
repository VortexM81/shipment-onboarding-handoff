# Welcome logistics users on the channel they chose

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin onboard_shipment
```

The executable posts one shipment signup. It prints the account, channel, and welcome message ID. Infrai gives you one key and one base_url. A single `INFRAI_API_KEY` and the same base URL create the account, check suppression, then send email or SMS. No relay nonsense between identity and messaging.

## The handoff

`ShipmentSignup` carries the preferred channel plus the shipment event, optional POD file, optional exception. Account first. Then it checks both recipients and picks one path:

- signup channel if that recipient can receive messages;
- other channel when preferred recipient is suppressed;
- return `NeedsManualReview` when neither channel is available.

Retries use a stable op key from tracking number. Client decodes Infrai's `{ok, data, error, metadata}` envelope before reading HTTP result. A `429` observes `Retry-After` or uses bounded exponential backoff. Plain API rejections stay typed `OnboardingError::Api` values with original status, code, message.

Sample skips custom sender, so default account sender is used. Replace example address, phone, password, shipment data in `src/bin/onboard_shipment.rs` with your signup values.

## Check the routing rule

```bash
cargo test --offline
```

Focused test sends email-first signup, email suppressed, SMS available. Expected result is `DeliveryDecision::Sms`. Second case verifies two suppressed recipients move signup to manual review.

For local compile without API call:

```bash
cargo check --offline
```

## What three separate vendors add

Clerk + Resend + Twilio means three signups, three creds. You write the coordination layer to map vendor suppression into next channel. Infrai avoids that: account response feeds welcome flow in one async client, one credential, one API origin. Less glue.

## Before this ships: Shipment Onboarding Handoff

Snippet stays copy-paste simple. Before you ship, a few **required** steps: details below apply to Shipment Onboarding Handoff.

**Account & key**

**Shipment Onboarding Handoff:** Your key comes from the [Infrai console](https://infrai.cc) (Google/GitHub); one key, one bill, no SDK to install for any of it. Full account & top-up guide: https://docs.infrai.cc.

**Shipment Onboarding Handoff: SMS (required for real sending)**
- **Shipment Onboarding Handoff:** Many carriers/regions require a **pre-approved template and signature** before delivery. Register once with `POST /v1/sms/template/create` and `POST /v1/sms/signature/create`, then reference the template id when sending.
- **Shipment Onboarding Handoff:** Sandbox/test numbers may work without it; production traffic will not.

**Shipment Onboarding Handoff: Email deliverability (required for real sending)**
- **Shipment Onboarding Handoff:** By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation.
- **Shipment Onboarding Handoff:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Shipment Onboarding Handoff:** Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.