//! Presentation metadata only; never an invitation token, session or key.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    derive(tsify::Tsify, serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    tsify(large_number_types_as_bigints, missing_as_null)
)]
pub enum OrganizationRoleView {
    Admin,
    User,
}
#[derive(Clone)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    derive(tsify::Tsify, serde::Serialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    tsify(large_number_types_as_bigints, missing_as_null)
)]
pub struct OrganizationMemberView {
    pub user_id: String,
    pub email: Option<String>,
    pub role: OrganizationRoleView,
    pub active: bool,
    pub revision: u64,
}
#[derive(Clone)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    derive(tsify::Tsify, serde::Serialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    tsify(large_number_types_as_bigints, missing_as_null)
)]
pub struct OrganizationOverviewView {
    pub company_id: String,
    pub name: String,
    pub revision: u64,
    pub member: OrganizationMemberView,
}
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    derive(tsify::Tsify, serde::Serialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    tsify(large_number_types_as_bigints, missing_as_null)
)]
pub enum OrganizationInvitationStatusView {
    Pending,
    Accepted,
    Expired,
    Revoked,
}
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    derive(tsify::Tsify, serde::Serialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    tsify(large_number_types_as_bigints, missing_as_null)
)]
pub enum OrganizationDeliveryView {
    Pending,
    Accepted,
    Failed,
    Unknown,
}
#[derive(Clone)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    derive(tsify::Tsify, serde::Serialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    tsify(large_number_types_as_bigints, missing_as_null)
)]
pub struct OrganizationInvitationView {
    pub id: String,
    pub issuer_user_id: String,
    pub email: String,
    pub role: OrganizationRoleView,
    pub expires_at_ms: u64,
    pub status: OrganizationInvitationStatusView,
    pub delivery: OrganizationDeliveryView,
}
#[derive(Clone)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    derive(tsify::Tsify, serde::Serialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    tsify(large_number_types_as_bigints, missing_as_null)
)]
pub struct OrganizationMemberPageView {
    pub members: Vec<OrganizationMemberView>,
    pub next_cursor: Option<String>,
}
#[derive(Clone)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    derive(tsify::Tsify, serde::Serialize)
)]
#[cfg_attr(
    all(target_arch = "wasm32", feature = "browser-bindings"),
    tsify(large_number_types_as_bigints, missing_as_null)
)]
pub struct OrganizationInvitationPageView {
    pub invitations: Vec<OrganizationInvitationView>,
    pub next_cursor: Option<String>,
}
pub enum OrganizationCommandResult {
    Invitation(OrganizationInvitationView),
    Member(OrganizationMemberView),
}
