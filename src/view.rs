//! Presentation metadata only; never an invitation token, session or key.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    User,
}
#[derive(Clone)]
pub struct Member {
    pub user_id: String,
    pub email: Option<String>,
    pub role: Role,
    pub active: bool,
    pub revision: u64,
}
#[derive(Clone)]
pub struct Overview {
    pub company_id: String,
    pub name: String,
    pub revision: u64,
    pub member: Member,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InvitationStatus {
    Pending,
    Accepted,
    Expired,
    Revoked,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Pending,
    Accepted,
    Failed,
    Unknown,
}
#[derive(Clone)]
pub struct Invitation {
    pub id: String,
    pub issuer_user_id: String,
    pub email: String,
    pub role: Role,
    pub expires_at_ms: u64,
    pub status: InvitationStatus,
    pub delivery: Delivery,
}
#[derive(Clone)]
pub struct MemberPage {
    pub members: Vec<Member>,
    pub next_cursor: Option<String>,
}
#[derive(Clone)]
pub struct InvitationPage {
    pub invitations: Vec<Invitation>,
    pub next_cursor: Option<String>,
}
pub enum CommandResult {
    Invitation(Invitation),
    Member(Member),
}
