# NDS Organizations working contract

This Rust module owns typed company client I/O. Core owns membership, roles,
command lifetime and authorization rules. Protocol owns wire DTOs; use the
Accounts generated projections and its closed signed transport. Never add a
second identity service, credential store, user database or frontend policy.
Read standarts.lock and module.yaml. Public AGPL-3.0-only code contains no real
identity, credentials, infrastructure configuration or runtime evidence.
Administrative commands are online, immutable, owner/device/lifetime-bound and
short-lived. Keep uncertain results explicit and retry the exact command; never
replace its ID/body, resend mail automatically or equate SMTP acceptance to join.
Run just check, WASM checks and actual owned backend/native acceptance. Signed
work -> dev -> main; no implicit deployment, migration or credential changes.
