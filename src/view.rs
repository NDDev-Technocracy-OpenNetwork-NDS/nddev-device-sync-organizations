//! Presentation metadata only; never an invitation token, session or key.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OrganizationRoleView {
    Admin,
    User,
}
#[derive(Clone)]
pub struct OrganizationMemberView {
    pub user_id: String,
    pub email: Option<String>,
    pub role: OrganizationRoleView,
    pub active: bool,
    pub revision: u64,
}
#[derive(Clone)]
pub struct OrganizationOverviewView {
    pub company_id: String,
    pub name: String,
    pub revision: u64,
    pub member: OrganizationMemberView,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OrganizationInvitationStatusView {
    Pending,
    Accepted,
    Expired,
    Revoked,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OrganizationDeliveryView {
    Pending,
    Accepted,
    Failed,
    Unknown,
}
#[derive(Clone)]
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
pub struct OrganizationMemberPageView {
    pub members: Vec<OrganizationMemberView>,
    pub next_cursor: Option<String>,
}
#[derive(Clone)]
pub struct OrganizationInvitationPageView {
    pub invitations: Vec<OrganizationInvitationView>,
    pub next_cursor: Option<String>,
}
pub enum OrganizationCommandResult {
    Invitation(OrganizationInvitationView),
    Member(OrganizationMemberView),
}
