use crate::{invalid, view};
use nddev_device_sync_accounts::{ClientError, organizations_wire as dto};
use nddev_device_sync_domain::{UserId, identity::EmailAddress, sync::MAX_REVISION};

pub(crate) fn revision(value: i64) -> Result<u64, ClientError> {
    u64::try_from(value)
        .ok()
        .filter(|value| (1..=MAX_REVISION).contains(value))
        .ok_or_else(invalid)
}
pub(crate) fn id(value: &str) -> Result<(), ClientError> {
    UserId::new(value.to_owned())
        .map(|_| ())
        .map_err(|_| invalid())
}
pub(crate) fn invitation_id(value: &str) -> Result<(), ClientError> {
    if value.len() == 43
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        Ok(())
    } else {
        Err(invalid())
    }
}
pub(crate) fn role(value: dto::Role) -> view::OrganizationRoleView {
    match value {
        dto::Role::Admin => view::OrganizationRoleView::Admin,
        dto::Role::User => view::OrganizationRoleView::User,
    }
}
pub(crate) fn wire_role(value: view::OrganizationRoleView) -> dto::Role {
    match value {
        view::OrganizationRoleView::Admin => dto::Role::Admin,
        view::OrganizationRoleView::User => dto::Role::User,
    }
}
pub(crate) fn member(
    value: dto::CompanyMember,
) -> Result<view::OrganizationMemberView, ClientError> {
    id(&value.user_id)?;
    if let Some(email) = &value.email {
        EmailAddress::parse(email).map_err(|_| invalid())?;
    }
    Ok(view::OrganizationMemberView {
        user_id: value.user_id,
        email: value.email,
        role: role(value.role),
        active: value.active,
        revision: revision(value.revision)?,
    })
}
pub(crate) fn invitation(
    value: dto::CompanyInvitation,
) -> Result<view::OrganizationInvitationView, ClientError> {
    invitation_id(&value.invitation_id)?;
    id(&value.issuer_user_id)?;
    EmailAddress::parse(&value.recipient_email).map_err(|_| invalid())?;
    Ok(view::OrganizationInvitationView {
        id: value.invitation_id,
        issuer_user_id: value.issuer_user_id,
        email: value.recipient_email,
        role: role(value.role),
        expires_at_ms: revision(value.expires_at_ms)?,
        status: match value.status {
            dto::Status::Pending => view::OrganizationInvitationStatusView::Pending,
            dto::Status::Accepted => view::OrganizationInvitationStatusView::Accepted,
            dto::Status::Expired => view::OrganizationInvitationStatusView::Expired,
            dto::Status::Revoked => view::OrganizationInvitationStatusView::Revoked,
        },
        delivery: match value.delivery {
            dto::Delivery::Pending => view::OrganizationDeliveryView::Pending,
            dto::Delivery::Accepted => view::OrganizationDeliveryView::Accepted,
            dto::Delivery::Failed => view::OrganizationDeliveryView::Failed,
            dto::Delivery::Unknown => view::OrganizationDeliveryView::Unknown,
        },
    })
}
/// An advancing, bounded keyset page; malicious or broken responses cannot make
/// the UI loop forever or combine duplicate identities.
pub(crate) fn page<'a>(
    ids: impl Iterator<Item = &'a str>,
    after: Option<&str>,
    next: Option<&str>,
) -> Result<(), ClientError> {
    // Cursor IDs are opaque: SQL collation need not be Rust byte ordering.
    let mut previous = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut count = 0;
    for id in ids {
        if Some(id) == after || !seen.insert(id) {
            return Err(invalid());
        }
        previous = Some(id);
        count += 1;
    }
    if count > nddev_device_sync_domain::organizations::commands::PAGE_SIZE
        || next.is_some_and(|next| count == 0 || Some(next) != previous)
    {
        return Err(invalid());
    }
    Ok(())
}

/// Public organization metadata retains the initiating principal separately
/// from the organization ID. Names never confer membership.
pub(crate) fn overview(
    value: dto::CompanyOverview,
    principal: &str,
) -> Result<view::OrganizationOverviewView, ClientError> {
    id(&value.company.company_id)?;
    nddev_device_sync_domain::organizations::CompanyName::new(value.company.display_name.clone())
        .map_err(|_| invalid())?;
    let member = member(value.member)?;
    if member.user_id != principal || !member.active {
        return Err(invalid());
    }
    Ok(view::OrganizationOverviewView {
        company_id: value.company.company_id,
        name: value.company.display_name,
        revision: revision(value.company.revision)?,
        member,
    })
}

pub(crate) fn context(
    value: &dto::OrganizationContext,
    principal: &str,
    company: &str,
    captured: u64,
) -> Result<(), ClientError> {
    if value.principal_id != principal
        || value.space.id != company
        || revision(value.membership_revision)? != captured
    {
        return Err(invalid());
    }
    Ok(())
}
