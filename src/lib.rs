//! Online personal/organization client. Accounts owns credentials and signing;
//! Core and the server check current membership and the captured revision.
mod codec;
mod scope;
pub mod view;
use nddev_device_sync_accounts::runtime;
use nddev_device_sync_accounts::{
    ClientError, IdentityClient, LocalUseFence, SyncRequest,
    company::{CompanyOperation, CompanyRequest},
    organizations_wire as dto,
};
use nddev_device_sync_domain::{
    identity::EmailAddress,
    organizations::{CompanyName, commands::CommandId},
};
use scope::{Principal, Selection};
use std::{future::Future, sync::Arc, time::Duration};
use tracing::Instrument;

fn invalid() -> ClientError {
    ClientError::InvalidResponse
}
fn now_ms() -> Result<u64, ClientError> {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000)
        .try_into()
        .map_err(|_| ClientError::InvalidInput)
}
// Only obtaining a local protected fence is retried. A submitted RPC or its
// post-response validation is never automatically replayed, even on local Busy.
async fn admitted<T, F: Future<Output = Result<T, ClientError>>>(
    mut work: impl FnMut() -> F,
) -> Result<T, ClientError> {
    let started = runtime::Instant::now();
    for attempt in 0..8 {
        match work().await {
            Err(ClientError::Busy) if attempt < 7 && started.elapsed() < Duration::from_secs(2) => {
                if attempt == 0 {
                    tracing::info!(target:"nddev_device_sync_organizations",module="organizations",scope="process",event.name="company.admission.waiting",outcome="pending");
                }
                runtime::sleep(Duration::from_millis((25_u64 << attempt).min(400))).await;
            }
            result => return result,
        }
    }
    Err(ClientError::Busy)
}

/// One original personal identity lifetime. Construct a new client after a
/// deliberate login transition; an old instance cannot borrow the new session.
pub struct OrganizationsClient {
    principal: Arc<Principal>,
}
/// An immutable selected organization and membership revision, not an account
/// or a Vault owner. Closing it denies its commands without closing other scopes.
pub struct Organization {
    selection: Arc<Selection>,
}
enum Target {
    Create,
    Invite,
    Revoke(String),
    Member(String),
}
enum CommandOwner {
    Personal {
        principal: Arc<Principal>,
        fence: Box<LocalUseFence>,
    },
    Organization(Arc<Selection>),
}
/// Original UUIDv7, exact bytes, personal fence and selected membership. Keep it
/// for an uncertain result. Only an explicit execute can replay these same bytes.
pub struct PreparedCommand {
    id: CommandId,
    body: Vec<u8>,
    target: Target,
    owner: CommandOwner,
}
impl Drop for OrganizationsClient {
    fn drop(&mut self) {
        self.close();
    }
}
impl Drop for Organization {
    fn drop(&mut self) {
        self.close();
    }
}
impl OrganizationsClient {
    pub fn new(identity: Arc<IdentityClient>) -> Self {
        Self {
            principal: Arc::new(Principal::new(identity)),
        }
    }
    pub fn close(&self) {
        self.principal.close();
    }
    /// Point-in-time lifetime check only. Composition calls it inside its own
    /// publication lock after awaited protected validation; it grants no access.
    pub fn check_publication(&self) -> Result<(), ClientError> {
        self.principal.publish(|| Ok(()))
    }
    pub async fn list(&self) -> Result<Vec<view::OrganizationOverviewView>, ClientError> {
        let fence = self.principal.fence().await?;
        let reply = self
            .principal
            .identity
            .sync_request(SyncRequest::Company(CompanyRequest::Organizations))
            .await;
        self.principal.validate(&fence).await?;
        let reply = reply?;
        if reply.status != 200 {
            return Err(invalid());
        }
        let value: dto::OrganizationList =
            serde_json::from_slice(&reply.body).map_err(|_| invalid())?;
        if value.context.principal_id != self.principal.user
            || value.context.space.id != self.principal.user
            // Defensive v3 OrganizationList.maxItems. This is a wire-size
            // rejection; the server/Core remains the membership authority.
            || value.organizations.len() > 32
        {
            return Err(invalid());
        }
        let mut ids = std::collections::BTreeSet::new();
        let organizations = value
            .organizations
            .into_iter()
            .map(|value| {
                if !ids.insert(value.company.company_id.clone()) {
                    return Err(invalid());
                }
                codec::overview(value, &self.principal.user)
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.principal.publish(|| {
            self.principal.identity.check_local_use_lifetime(&fence)?;
            Ok(organizations)
        })
    }
    /// The caller supplies the revision it actually saw in list(). The server
    /// must still confirm it; a stale selection never silently adopts a rejoin.
    pub async fn select(
        &self,
        company: String,
        revision: u64,
    ) -> Result<Organization, ClientError> {
        codec::id(&company).map_err(|_| ClientError::InvalidInput)?;
        codec::revision(i64::try_from(revision).map_err(|_| ClientError::InvalidInput)?)
            .map_err(|_| ClientError::InvalidInput)?;
        let fence = self.principal.fence().await?;
        let selected = Organization {
            selection: Arc::new(Selection::new(
                self.principal.clone(),
                fence,
                company,
                revision,
            )),
        };
        selected.overview().await?;
        selected.check_publication()?;
        Ok(selected)
    }
    pub async fn create(&self, name: String) -> Result<PreparedCommand, ClientError> {
        let name = CompanyName::new(name).map_err(|_| ClientError::InvalidInput)?;
        let fence = self.principal.fence().await?;
        let id = CommandId::new(uuid::Uuid::now_v7().to_string())
            .map_err(|_| ClientError::InvalidInput)?;
        let body = serde_json::to_vec(&dto::OrganizationCreateRequest {
            display_name: name.as_str().into(),
            operation_id: id.as_str().into(),
        })
        .map_err(|_| ClientError::InvalidInput)?;
        self.principal
            .publish(|| self.principal.identity.check_local_use_lifetime(&fence))?;
        Ok(PreparedCommand {
            id,
            body,
            target: Target::Create,
            owner: CommandOwner::Personal {
                principal: self.principal.clone(),
                fence: Box::new(fence),
            },
        })
    }
}
impl Organization {
    pub fn close(&self) {
        self.selection.close();
    }
    pub fn check_publication(&self) -> Result<(), ClientError> {
        self.selection.publish(|| Ok(()))
    }
    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        operation: CompanyOperation,
    ) -> Result<T, ClientError> {
        self.selection.validate().await?;
        let reply = self
            .selection
            .principal
            .identity
            .sync_request(SyncRequest::Company(CompanyRequest::Scoped {
                company: self.selection.company.clone(),
                revision: self.selection.revision,
                operation,
            }))
            .await;
        let reply = self.selection.denied(reply)?;
        self.selection.validate().await?;
        if reply.status != 200 {
            return Err(invalid());
        }
        serde_json::from_slice(&reply.body).map_err(|_| invalid())
    }
    pub async fn overview(&self) -> Result<view::OrganizationOverviewView, ClientError> {
        let value: dto::OrganizationOverview = self.read(CompanyOperation::Overview).await?;
        self.selection.context(&value.context)?;
        let view = codec::overview(value.overview, &self.selection.principal.user)?;
        if view.company_id != self.selection.company
            || view.member.revision != self.selection.revision
        {
            return Err(invalid());
        }
        self.selection.publish(|| Ok(view))
    }
    pub async fn members(
        &self,
        after: Option<String>,
    ) -> Result<view::OrganizationMemberPageView, ClientError> {
        let value: dto::OrganizationMemberPage = self
            .read(CompanyOperation::Members {
                after: after.clone(),
            })
            .await?;
        self.selection.context(&value.context)?;
        codec::page(
            value
                .page
                .members
                .iter()
                .map(|member| member.user_id.as_str()),
            after.as_deref(),
            value.page.next_cursor.as_deref(),
        )?;
        let view = view::OrganizationMemberPageView {
            members: value
                .page
                .members
                .into_iter()
                .map(codec::member)
                .collect::<Result<_, _>>()?,
            next_cursor: value.page.next_cursor,
        };
        self.selection.publish(|| Ok(view))
    }
    pub async fn invitations(
        &self,
        after: Option<String>,
    ) -> Result<view::OrganizationInvitationPageView, ClientError> {
        let value: dto::OrganizationInvitationPage = self
            .read(CompanyOperation::Invitations {
                after: after.clone(),
            })
            .await?;
        self.selection.context(&value.context)?;
        codec::page(
            value
                .page
                .invitations
                .iter()
                .map(|invite| invite.invitation_id.as_str()),
            after.as_deref(),
            value.page.next_cursor.as_deref(),
        )?;
        let view = view::OrganizationInvitationPageView {
            invitations: value
                .page
                .invitations
                .into_iter()
                .map(codec::invitation)
                .collect::<Result<_, _>>()?,
            next_cursor: value.page.next_cursor,
        };
        self.selection.publish(|| Ok(view))
    }
    async fn prepare(
        &self,
        target: Target,
        body: impl FnOnce(String, i64) -> Result<Vec<u8>, ClientError>,
    ) -> Result<PreparedCommand, ClientError> {
        self.selection.validate().await?;
        let id = CommandId::new(uuid::Uuid::now_v7().to_string())
            .map_err(|_| ClientError::InvalidInput)?;
        let body = body(id.as_str().into(), self.selection.revision as i64)?;
        self.selection.publish(|| {
            Ok(PreparedCommand {
                id,
                body,
                target,
                owner: CommandOwner::Organization(self.selection.clone()),
            })
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
        self.prepare(Target::Invite, |operation_id, membership_revision| {
            serde_json::to_vec(&dto::OrganizationInvitationRequest {
                membership_revision,
                payload: dto::CompanyInvitationRequest {
                    email,
                    role: codec::wire_role(role),
                    operation_id,
                },
            })
            .map_err(|_| ClientError::InvalidInput)
        })
        .await
    }
    pub async fn revoke(&self, invitation: String) -> Result<PreparedCommand, ClientError> {
        codec::invitation_id(&invitation).map_err(|_| ClientError::InvalidInput)?;
        self.prepare(
            Target::Revoke(invitation),
            |operation_id, membership_revision| {
                serde_json::to_vec(&dto::OrganizationRevokeRequest {
                    membership_revision,
                    payload: dto::CompanyRevokeRequest { operation_id },
                })
                .map_err(|_| ClientError::InvalidInput)
            },
        )
        .await
    }
    pub async fn change_member(
        &self,
        user: String,
        revision: u64,
        role: Option<view::OrganizationRoleView>,
    ) -> Result<PreparedCommand, ClientError> {
        codec::id(&user).map_err(|_| ClientError::InvalidInput)?;
        let revision = i64::try_from(revision).map_err(|_| ClientError::InvalidInput)?;
        codec::revision(revision).map_err(|_| ClientError::InvalidInput)?;
        self.prepare(Target::Member(user), |operation_id, membership_revision| {
            serde_json::to_vec(&dto::OrganizationMemberChangeRequest {
                membership_revision,
                payload: dto::CompanyMemberChangeRequest {
                    operation_id,
                    expected_revision: revision,
                    action: if role.is_some() {
                        dto::Action::SetRole
                    } else {
                        dto::Action::Remove
                    },
                    role: role.map(codec::wire_role),
                },
            })
            .map_err(|_| ClientError::InvalidInput)
        })
        .await
    }
}
impl CommandOwner {
    fn principal(&self) -> &Principal {
        match self {
            Self::Personal { principal, .. } => principal,
            Self::Organization(selected) => &selected.principal,
        }
    }
    fn fence(&self) -> &LocalUseFence {
        match self {
            Self::Personal { fence, .. } => fence,
            Self::Organization(selected) => &selected.fence,
        }
    }
    fn publish<T>(&self, work: impl FnOnce() -> Result<T, ClientError>) -> Result<T, ClientError> {
        match self {
            Self::Personal { principal, fence } => principal.publish(|| {
                principal.identity.check_local_use_lifetime(fence)?;
                work()
            }),
            Self::Organization(selected) => selected.publish(work),
        }
    }
    async fn validate(&self) -> Result<(), ClientError> {
        self.publish(|| Ok(()))?;
        self.principal().validate(self.fence()).await?;
        self.publish(|| Ok(()))
    }
}
impl PreparedCommand {
    pub fn check_publication(&self) -> Result<(), ClientError> {
        self.owner.publish(|| Ok(()))
    }
    pub async fn execute(&self) -> Result<view::OrganizationCommandResult, ClientError> {
        let span = nddev_device_sync_telemetry::operation_span("organizations", "http")
            .map_err(|_| ClientError::TelemetryUnavailable)?;
        async {
            let result=self.execute_inner().await;
            let principal=self.owner.principal();
            let tenant=match &self.owner {CommandOwner::Personal{..}=>principal.storage_tenant.as_str(),CommandOwner::Organization(selected)=>selected.company.as_str()};
            match &result {
                Ok(_)=>tracing::event!(target:"nddev_device_sync_organizations",tracing::Level::INFO,
                    {event.name="company.command.completed",operation.id=self.id.as_str(),user.id=principal.user.as_str(),tenant.id=tenant,device.id=principal.device.as_str(),outcome="ok"}),
                Err(error)=>tracing::event!(target:"nddev_device_sync_organizations",tracing::Level::WARN,
                    {event.name="company.command.completed",operation.id=self.id.as_str(),user.id=principal.user.as_str(),tenant.id=tenant,device.id=principal.device.as_str(),outcome="error",error.type=error.code()}),
            }
            result
        }.instrument(span).await
    }
    async fn execute_inner(&self) -> Result<view::OrganizationCommandResult, ClientError> {
        self.id
            .validate(now_ms()?)
            .map_err(|_| ClientError::Server("command_expired".into()))?;
        self.owner.validate().await?;
        let request = match (&self.target, &self.owner) {
            (Target::Create, CommandOwner::Personal { .. }) => {
                CompanyRequest::Create(self.body.clone())
            }
            (target, CommandOwner::Organization(selected)) => CompanyRequest::Scoped {
                company: selected.company.clone(),
                revision: selected.revision,
                operation: match target {
                    Target::Invite => CompanyOperation::Invite(self.body.clone()),
                    Target::Revoke(id) => CompanyOperation::Revoke {
                        id: id.clone(),
                        body: self.body.clone(),
                    },
                    Target::Member(id) => CompanyOperation::ChangeMember {
                        id: id.clone(),
                        body: self.body.clone(),
                    },
                    Target::Create => return Err(ClientError::InvalidInput),
                },
            },
            _ => return Err(ClientError::InvalidInput),
        };
        let reply = self
            .owner
            .principal()
            .identity
            .sync_request(SyncRequest::Company(request))
            .await;
        let reply = match &self.owner {
            CommandOwner::Organization(selected) => selected.denied(reply),
            _ => reply,
        }?;
        self.owner.validate().await?;
        let expected = if matches!(self.target, Target::Create | Target::Invite) {
            201
        } else {
            200
        };
        if reply.status != expected {
            return Err(invalid());
        }
        let result = match &self.target {
            Target::Create => {
                let value: dto::OrganizationOverview =
                    serde_json::from_slice(&reply.body).map_err(|_| invalid())?;
                let view = codec::overview(value.overview, &self.owner.principal().user)?;
                if value.context.principal_id != self.owner.principal().user
                    || value.context.space.id != view.company_id
                    || codec::revision(value.context.membership_revision)? != view.member.revision
                    || view.member.role != view::OrganizationRoleView::Admin
                {
                    return Err(invalid());
                }
                view::OrganizationCommandResult::Organization(view)
            }
            Target::Member(id) => {
                let value: dto::OrganizationMemberResult =
                    serde_json::from_slice(&reply.body).map_err(|_| invalid())?;
                let CommandOwner::Organization(selected) = &self.owner else {
                    return Err(invalid());
                };
                selected.context(&value.context)?;
                let member = codec::member(value.member)?;
                if &member.user_id != id {
                    return Err(invalid());
                }
                view::OrganizationCommandResult::Member(member)
            }
            target => {
                let value: dto::OrganizationInvitationResult =
                    serde_json::from_slice(&reply.body).map_err(|_| invalid())?;
                let CommandOwner::Organization(selected) = &self.owner else {
                    return Err(invalid());
                };
                selected.context(&value.context)?;
                let invitation = codec::invitation(value.invitation)?;
                if matches!(target,Target::Revoke(id) if id!=&invitation.id) {
                    return Err(invalid());
                }
                view::OrganizationCommandResult::Invitation(invitation)
            }
        };
        self.owner.publish(|| Ok(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_admission_wait_is_bounded_and_accepted_by_the_actual_sdk() {
        use opentelemetry::trace::TracerProvider;
        use std::{io::Write, sync::Mutex};
        #[derive(Clone)]
        struct Output(Arc<Mutex<Vec<u8>>>);
        impl Write for Output {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let output = Output(Arc::default());
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let (subscriber, control) = nddev_device_sync_telemetry::subscriber(
            nddev_device_sync_telemetry::Config {
                service: "organizations-acceptance".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                repository: "NDDev-Technocracy-OpenNetwork-NDS/nddev-device-sync-organizations"
                    .into(),
                commit: "unknown".into(),
                channel: "alpha".into(),
                standards: "unreleased".into(),
                environment: "test".into(),
                target_prefixes: vec!["nddev_device_sync_organizations".into()],
                default_module: "process".into(),
                debug: None,
            },
            output.clone(),
            provider.tracer("organizations-acceptance"),
        )
        .unwrap();
        tracing::subscriber::with_default(subscriber, || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let mut attempts = 0;
                let result: Result<(), _> = admitted(|| {
                    attempts += 1;
                    std::future::ready(Err(ClientError::Busy))
                })
                .await;
                assert_eq!(result, Err(ClientError::Busy));
                assert_eq!(attempts, 8);
                let mut attempts = 0;
                let result: Result<(), _> = admitted(|| {
                    attempts += 1;
                    std::future::ready(Err(ClientError::Transport))
                })
                .await;
                assert_eq!(result, Err(ClientError::Transport));
                assert_eq!(attempts, 1);
            });
        });
        assert_eq!(control.rejected_fields_or_events(), 0);
        let bytes = output.0.lock().unwrap();
        let events: Vec<serde_json::Value> = std::str::from_utf8(&bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let waiting: Vec<_> = events
            .iter()
            .filter(|event| event["event.name"] == "company.admission.waiting")
            .collect();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0]["outcome"], "pending");
        assert_eq!(waiting[0]["module"], "organizations");
    }

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
    #[test]
    fn original_client_lifetime_closure_cannot_borrow_a_replacement() {
        let identity = Arc::new(IdentityClient::new("https://example.invalid").unwrap());
        let original = OrganizationsClient::new(identity.clone());
        assert!(original.check_publication().is_ok()); // lifetime only, no grant
        identity.cancel();
        assert_eq!(
            original.check_publication(),
            Err(ClientError::RevalidationRequired)
        );
        let replacement = OrganizationsClient::new(identity);
        assert!(replacement.check_publication().is_ok());
        original.close();
        assert!(replacement.check_publication().is_ok());
        replacement.close();
        assert_eq!(
            replacement
                .principal
                .publish(|| panic!("closed client published")),
            Err::<(), _>(ClientError::RevalidationRequired)
        );
    }
    #[test]
    fn organization_context_requires_original_actor_space_and_revision_together() {
        let context = dto::OrganizationContext {
            principal_id: "principal".into(),
            membership_revision: 7,
            space: dto::OrganizationSpace {
                id: "organization".into(),
                kind: dto::OrganizationSpaceKind::Organization,
            },
        };
        assert!(codec::context(&context, "principal", "organization", 7).is_ok());
        for (actor, space, revision) in [
            ("other", "organization", 7),
            ("principal", "other", 7),
            ("principal", "organization", 8),
            ("organization", "principal", 7),
        ] {
            assert_eq!(
                codec::context(&context, actor, space, revision),
                Err(ClientError::InvalidResponse)
            );
        }
        let mut invalid = context.clone();
        invalid.membership_revision = 0;
        assert!(codec::context(&invalid, "principal", "organization", 0).is_err());
        let overview = |company: &str, principal: &str| dto::CompanyOverview {
            company: dto::Company {
                company_id: company.into(),
                display_name: "Same name".into(),
                revision: 1,
            },
            member: dto::CompanyMember {
                user_id: principal.into(),
                email: None,
                role: dto::Role::User,
                active: true,
                revision: 7,
            },
        };
        let first = codec::overview(overview("first", "principal"), "principal").unwrap();
        let second = codec::overview(overview("second", "principal"), "principal").unwrap();
        assert_eq!(first.name, second.name);
        assert_ne!(first.company_id, second.company_id);
        assert!(codec::overview(overview("first", "other"), "principal").is_err());
    }
}
