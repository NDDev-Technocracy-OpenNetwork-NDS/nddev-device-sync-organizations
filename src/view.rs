//! Presentation metadata only; never an invitation token, session or key.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CompanyRoleView {
    Admin,
    User,
}
#[derive(Clone)]
pub struct CompanyMemberView {
    pub user_id: String,
    pub email: Option<String>,
    pub role: CompanyRoleView,
    pub active: bool,
    pub revision: u64,
}
#[derive(Clone)]
pub struct CompanyOverview {
    pub company_id: String,
    pub name: String,
    pub revision: u64,
    pub member: CompanyMemberView,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CompanyInvitationStatus {
    Pending,
    Accepted,
    Expired,
    Revoked,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CompanyDeliveryView {
    Pending,
    Accepted,
    Failed,
    Unknown,
}
#[derive(Clone)]
pub struct CompanyInvitationView {
    pub id: String,
    pub issuer_user_id: String,
    pub email: String,
    pub role: CompanyRoleView,
    pub expires_at_ms: u64,
    pub status: CompanyInvitationStatus,
    pub delivery: CompanyDeliveryView,
}
#[derive(Clone)]
pub struct CompanyMemberPage {
    pub members: Vec<CompanyMemberView>,
    pub next_cursor: Option<String>,
}
#[derive(Clone)]
pub struct CompanyInvitationPage {
    pub invitations: Vec<CompanyInvitationView>,
    pub next_cursor: Option<String>,
}
pub enum CompanyCommandResult {
    Invitation(CompanyInvitationView),
    Member(CompanyMemberView),
}
