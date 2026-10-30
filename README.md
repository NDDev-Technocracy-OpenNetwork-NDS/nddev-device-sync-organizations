# NDS Organizations

Rust company client for NDDev OpenNetwork. AGPL-3.0-only.

Core owns company membership and command rules; the existing NDS server enforces
current authority. Accounts owns session credentials, enrolled-device signing and
the generated Protocol DTOs. This module composes those owners for company
metadata, a personal organization list, explicit organization creation/selection,
bounded member/invitation pages, invitations, role changes and removal.
It creates no separate user store, credentials or authorization service.

The original personal principal, storage owner, device keys and Vault remain
unchanged by organization selection or membership changes. `list()` returns
current memberships. `select(company_id, membership_revision)` verifies the
exact observed membership online and returns one immutable `Organization`.
Each scoped read and command carries that revision; removal, role change or
rejoin cannot lend a new grant to old work. Closing a selection denies only that
selection. Closing its `OrganizationsClient` closes all work of that original
personal lifetime. Neither operation logs out the personal account. A denied
selection must be deliberately selected again from current server metadata;
there is no fallback to the original company or another organization. Old
servers without this v3 contract refuse explicitly.

Administrative commands are online. Keep the opaque `PreparedCommand` while an
outcome is uncertain and call its `execute` again: the original UUIDv7/body/target
remain unchanged, while Accounts supplies fresh proof and checks the same local
identity lifetime. Commands expire after five minutes; expiration does not prove
an earlier attempt failed. Refresh authoritative state before a deliberate new
command. Closing/restarting does not replay or automatically resend mail. There
is no offline administrative queue. Identity links and email verification belong
to Accounts; no invitation tokens enter presentation DTOs.

Client diagnostic actor fields keep the original Accounts owner/device tuple,
which the signed telemetry gateway verifies. The selected organization and
membership revision remain in the signed operation context and trusted server
audit. This adds no organization-wide diagnostic query or global admin grant.

The client distinguishes invitation state from mail delivery state. A successful
creation receipt does not assert delivery or membership activation. Personal
Vault, secrets and repositories remain private to their owners.

Run `just check wasm-check`. `just backend-acceptance` uses an explicitly started
isolated server PostgreSQL/Mailpit fixture (`NDS_FIXTURE_RECEIPT`) and native
credential service. It exercises actual signing, invitation replay/revoke/email
verification, account switching, role revision conflicts and last-admin denial.
Unit tests or WASM compilation alone are not platform/runtime qualification.

Signed work → dev → main. No implicit release or live deployment.

Local `busy` contention with background work has at most eight admission attempts
with a bounded 25–400 ms delay while obtaining the original protected fence.
Submitted RPCs and post-response validation are never automatically replayed,
including on local `busy`. The same captured identity lifetime and immutable
command are retained for a deliberate retry. Closing a caller or a late
validation refusal does not establish that a submitted mutation had no effect.
This does not add an operation queue or stop sync.

Release journals live in [GitHub Releases](https://github.com/NDDev-Technocracy-OpenNetwork-NDS/nddev-device-sync-organizations/releases).
