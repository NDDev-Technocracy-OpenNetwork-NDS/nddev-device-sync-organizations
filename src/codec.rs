use crate::{invalid, view};
use nddev_device_sync_accounts::{ClientError, company_wire as dto};
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
pub(crate) fn role(value: dto::Role) -> view::CompanyRoleView {
    match value {
        dto::Role::Admin => view::CompanyRoleView::Admin,
        dto::Role::User => view::CompanyRoleView::User,
    }
}
pub(crate) fn wire_role(value: view::CompanyRoleView) -> dto::Role {
    match value {
        view::CompanyRoleView::Admin => dto::Role::Admin,
        view::CompanyRoleView::User => dto::Role::User,
    }
}
pub(crate) fn member(value: dto::CompanyMember) -> Result<view::CompanyMemberView, ClientError> {
    id(&value.user_id)?;
    if let Some(email) = &value.email {
        EmailAddress::parse(email).map_err(|_| invalid())?;
    }
    Ok(view::CompanyMemberView {
        user_id: value.user_id,
        email: value.email,
        role: role(value.role),
        active: value.active,
        revision: revision(value.revision)?,
    })
}
pub(crate) fn invitation(
    value: dto::CompanyInvitation,
) -> Result<view::CompanyInvitationView, ClientError> {
    invitation_id(&value.invitation_id)?;
    id(&value.issuer_user_id)?;
    EmailAddress::parse(&value.recipient_email).map_err(|_| invalid())?;
    Ok(view::CompanyInvitationView {
        id: value.invitation_id,
        issuer_user_id: value.issuer_user_id,
        email: value.recipient_email,
        role: role(value.role),
        expires_at_ms: revision(value.expires_at_ms)?,
        status: match value.status {
            dto::Status::Pending => view::CompanyInvitationStatus::Pending,
            dto::Status::Accepted => view::CompanyInvitationStatus::Accepted,
            dto::Status::Expired => view::CompanyInvitationStatus::Expired,
            dto::Status::Revoked => view::CompanyInvitationStatus::Revoked,
        },
        delivery: match value.delivery {
            dto::Delivery::Pending => view::CompanyDeliveryView::Pending,
            dto::Delivery::Accepted => view::CompanyDeliveryView::Accepted,
            dto::Delivery::Failed => view::CompanyDeliveryView::Failed,
            dto::Delivery::Unknown => view::CompanyDeliveryView::Unknown,
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
    let mut previous = after;
    let mut count = 0;
    for id in ids {
        if previous.is_some_and(|previous| previous >= id) {
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
