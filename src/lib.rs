//! Online company client. One credential/signing authority: Accounts. Server
//! Core policies decide access on every attempt, including idempotent replays.
mod codec;
pub mod view;
use nddev_device_sync_accounts::{
    ClientError, IdentityClient, LocalUseFence, SyncRequest, company::CompanyRequest,
    company_wire as dto,
};
use nddev_device_sync_domain::{
    identity::EmailAddress,
    organizations::{CompanyName, commands::CommandId},
};
use std::sync::Arc;
use tracing::Instrument;

fn invalid() -> ClientError {
    ClientError::InvalidResponse
}
fn now_ms() -> Result<u64, ClientError> {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000)
        .try_into()
        .map_err(|_| ClientError::InvalidInput)
}
pub struct OrganizationsClient {
    identity: Arc<IdentityClient>,
}
enum Target {
    Invite,
    Revoke(String),
    Member(String),
}
/// Retain this opaque Rust object for an uncertain result. It cannot be edited
/// or serialized by a frontend. Closing/restarting does not replay it; refresh
/// authoritative state before deliberately submitting a new command.
pub struct PreparedCommand {
    id: CommandId,
    body: Vec<u8>,
    target: Target,
    fence: LocalUseFence,
    user: String,
    tenant: String,
    device: String,
}
impl OrganizationsClient {
    pub fn new(identity: Arc<IdentityClient>) -> Self {
        Self { identity }
    }
    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        request: CompanyRequest,
    ) -> Result<T, ClientError> {
        let fence = self.identity.local_use_fence().await?;
        let reply = self
            .identity
            .sync_request(SyncRequest::Company(request))
            .await?;
        self.identity.validate_local_use(&fence).await?;
        if reply.status != 200 {
            return Err(invalid());
        }
        serde_json::from_slice(&reply.body).map_err(|_| invalid())
    }
    pub async fn overview(&self) -> Result<view::OrganizationOverviewView, ClientError> {
        let scope = self.identity.view();
        let result: dto::CompanyOverview = self.read(CompanyRequest::Overview).await?;
        if result.company.company_id != scope.tenant_id
            || result.member.user_id != scope.user_id
            || !result.member.active
        {
            return Err(invalid());
        }
        CompanyName::new(result.company.display_name.clone()).map_err(|_| invalid())?;
        Ok(view::OrganizationOverviewView {
            company_id: result.company.company_id,
            name: result.company.display_name,
            revision: codec::revision(result.company.revision)?,
            member: codec::member(result.member)?,
        })
    }
    pub async fn members(
        &self,
        after: Option<String>,
    ) -> Result<view::OrganizationMemberPageView, ClientError> {
        let value: dto::CompanyMemberPage = self
            .read(CompanyRequest::Members {
                after: after.clone(),
            })
            .await?;
        codec::page(
            value.members.iter().map(|member| member.user_id.as_str()),
            after.as_deref(),
            value.next_cursor.as_deref(),
        )?;
        Ok(view::OrganizationMemberPageView {
            members: value
                .members
                .into_iter()
                .map(codec::member)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
    pub async fn invitations(
        &self,
        after: Option<String>,
    ) -> Result<view::OrganizationInvitationPageView, ClientError> {
        let value: dto::CompanyInvitationPage = self
            .read(CompanyRequest::Invitations {
                after: after.clone(),
            })
            .await?;
        codec::page(
            value
                .invitations
                .iter()
                .map(|invite| invite.invitation_id.as_str()),
            after.as_deref(),
            value.next_cursor.as_deref(),
        )?;
        Ok(view::OrganizationInvitationPageView {
            invitations: value
                .invitations
                .into_iter()
                .map(codec::invitation)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
    async fn prepare(
        &self,
        target: Target,
        body: impl FnOnce(String) -> Result<Vec<u8>, ClientError>,
    ) -> Result<PreparedCommand, ClientError> {
        let fence = self.identity.local_use_fence().await?;
        let scope = self.identity.view();
        let id = CommandId::new(uuid::Uuid::now_v7().to_string())
            .map_err(|_| ClientError::InvalidInput)?;
        let body = body(id.as_str().into())?;
        Ok(PreparedCommand {
            id,
            body,
            target,
            fence,
            user: scope.user_id,
            tenant: scope.tenant_id,
            device: scope.current_device_id,
        })
    }
    pub async fn invite(
        &self,
        email: String,
        role: view::OrganizationRoleView,
    ) -> Result<PreparedCommand, ClientError> {
        let email = EmailAddress::parse(&email)
            .map_err(|_| ClientError::InvalidInput)?
            .as_str()
            .to_owned();
        self.prepare(Target::Invite, |operation_id| {
            serde_json::to_vec(&dto::CompanyInvitationRequest {
                email,
                role: codec::wire_role(role),
                operation_id,
            })
            .map_err(|_| ClientError::InvalidInput)
        })
        .await
    }
    pub async fn revoke(&self, invitation: String) -> Result<PreparedCommand, ClientError> {
        codec::invitation_id(&invitation).map_err(|_| ClientError::InvalidInput)?;
        self.prepare(Target::Revoke(invitation), |operation_id| {
            serde_json::to_vec(&dto::CompanyRevokeRequest { operation_id })
                .map_err(|_| ClientError::InvalidInput)
        })
        .await
    }
    /// None removes membership; Some changes its role at the observed revision.
    pub async fn change_member(
        &self,
        user: String,
        revision: u64,
        role: Option<view::OrganizationRoleView>,
    ) -> Result<PreparedCommand, ClientError> {
        codec::id(&user).map_err(|_| ClientError::InvalidInput)?;
        let revision = i64::try_from(revision).map_err(|_| ClientError::InvalidInput)?;
        codec::revision(revision).map_err(|_| ClientError::InvalidInput)?;
        self.prepare(Target::Member(user), |operation_id| {
            serde_json::to_vec(&dto::CompanyMemberChangeRequest {
                operation_id,
                expected_revision: revision,
                action: if role.is_some() {
                    dto::Action::SetRole
                } else {
                    dto::Action::Remove
                },
                role: role.map(codec::wire_role),
            })
            .map_err(|_| ClientError::InvalidInput)
        })
        .await
    }
    pub async fn execute(
        &self,
        command: &PreparedCommand,
    ) -> Result<view::OrganizationCommandResult, ClientError> {
        let span = nddev_device_sync_telemetry::operation_span("organizations", "http")
            .map_err(|_| ClientError::TelemetryUnavailable)?;
        async {
            let result = self.execute_inner(command).await;
            match &result {
                Ok(_) => tracing::event!(target: "nddev_device_sync_organizations", tracing::Level::INFO,
                    { event.name = "company.command.completed", operation.id = command.id.as_str(),
                      user.id = command.user.as_str(), tenant.id = command.tenant.as_str(), device.id = command.device.as_str(), outcome = "ok" }),
                Err(error) => tracing::event!(target: "nddev_device_sync_organizations", tracing::Level::WARN,
                    { event.name = "company.command.completed", operation.id = command.id.as_str(),
                      user.id = command.user.as_str(), tenant.id = command.tenant.as_str(), device.id = command.device.as_str(),
                      outcome = "error", error.type = error.code() }),
            }
            result
        }.instrument(span).await
    }
    async fn execute_inner(
        &self,
        command: &PreparedCommand,
    ) -> Result<view::OrganizationCommandResult, ClientError> {
        command
            .id
            .validate(now_ms()?)
            .map_err(|_| ClientError::Server("command_expired".into()))?;
        self.identity.validate_local_use(&command.fence).await?;
        let request = match &command.target {
            Target::Invite => CompanyRequest::Invite(command.body.clone()),
            Target::Revoke(id) => CompanyRequest::Revoke {
                id: id.clone(),
                body: command.body.clone(),
            },
            Target::Member(id) => CompanyRequest::ChangeMember {
                id: id.clone(),
                body: command.body.clone(),
            },
        };
        let result = self
            .identity
            .sync_request(SyncRequest::Company(request))
            .await;
        self.identity.validate_local_use(&command.fence).await?;
        let reply = result?;
        if reply.status
            != if matches!(command.target, Target::Invite) {
                201
            } else {
                200
            }
        {
            return Err(invalid());
        }
        match &command.target {
            Target::Member(id) => {
                let member =
                    codec::member(serde_json::from_slice(&reply.body).map_err(|_| invalid())?)?;
                if &member.user_id != id {
                    return Err(invalid());
                }
                Ok(view::OrganizationCommandResult::Member(member))
            }
            target => {
                let invitation =
                    codec::invitation(serde_json::from_slice(&reply.body).map_err(|_| invalid())?)?;
                if matches!(target, Target::Revoke(id) if id != &invitation.id) {
                    return Err(invalid());
                }
                Ok(view::OrganizationCommandResult::Invitation(invitation))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_reject_nonadvancing_duplicates_and_unbounded_results() {
        assert!(codec::page(["b", "c"].into_iter(), Some("a"), Some("c")).is_ok());
        assert!(codec::page(["a"].into_iter(), Some("a"), None).is_err());
        assert!(codec::page(["b", "A"].into_iter(), None, Some("A")).is_ok());
        let oversized: Vec<String> = (0..51).map(|value| format!("id{value}")).collect();
        assert!(codec::page(oversized.iter().map(String::as_str), None, None).is_err());
        assert!(codec::page(["b", "b"].into_iter(), None, None).is_err());
        assert!(codec::page(["b"].into_iter(), None, Some("z")).is_err());
        assert!(codec::page([].into_iter(), None, Some("z")).is_err());
    }
    #[test]
    fn response_metadata_never_accepts_invalid_revision_or_email() {
        for revision in [0, -1, i64::MAX] {
            assert!(codec::revision(revision).is_err());
        }
        let member = dto::CompanyMember {
            user_id: "user".into(),
            email: Some("invalid".into()),
            role: dto::Role::Admin,
            active: true,
            revision: 1,
        };
        assert!(codec::member(member).is_err());
    }
}
