//! Uses only the public Rust clients and an explicitly owned PG/SMTP fixture.
use nddev_device_sync_accounts::IdentityClient;
use nddev_device_sync_organizations::{
    OrganizationsClient,
    view::{OrganizationCommandResult, OrganizationInvitationStatusView, OrganizationRoleView},
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

async fn messages(http: &reqwest::Client, mailbox: &str) -> Vec<Value> {
    let value: Value = http
        .get(format!("{mailbox}/api/v1/messages"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    value["messages"].as_array().unwrap().clone()
}
async fn mail(
    http: &reqwest::Client,
    mailbox: &str,
    email: &str,
    prior: &[Value],
    marker: &str,
) -> String {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            for row in messages(http, mailbox).await {
                if prior.iter().any(|old| old["ID"] == row["ID"]) {
                    continue;
                }
                let message: Value = http
                    .get(format!(
                        "{mailbox}/api/v1/message/{}",
                        row["ID"].as_str().unwrap()
                    ))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                if !message["To"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|to| to["Address"] == email)
                {
                    continue;
                }
                let text = message["Text"].as_str().unwrap();
                if text.contains(marker) {
                    return text.to_owned();
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap()
}
async fn login(identity: &IdentityClient, http: &reqwest::Client, mailbox: &str, email: &str) {
    let prior = messages(http, mailbox).await;
    identity.request_email(email.into(), "en").await.unwrap();
    let text = mail(http, mailbox, email, &prior, "code: ").await;
    let code = text
        .split_once("code: ")
        .unwrap()
        .1
        .chars()
        .take(8)
        .collect();
    identity.verify_email(code).await.unwrap();
    identity
        .prepare_device("Owned company client".into())
        .await
        .unwrap();
}
fn invitation(
    result: OrganizationCommandResult,
) -> nddev_device_sync_organizations::view::OrganizationInvitationView {
    match result {
        OrganizationCommandResult::Invitation(value) => value,
        _ => panic!("expected invitation"),
    }
}
#[tokio::test]
#[ignore = "requires an owned PostgreSQL/Mailpit fixture and isolated native credential service"]
async fn actual_company_management_and_invitation_login() {
    let receipt = std::env::var("NDS_FIXTURE_RECEIPT").unwrap();
    let fixture: Value = serde_json::from_slice(&std::fs::read(receipt).unwrap()).unwrap();
    let endpoint = fixture["base_url"].as_str().unwrap();
    let mailbox = fixture["mailpit_url"].as_str().unwrap();
    let owner = fixture["owner_email"].as_str().unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:") && mailbox.starts_with("http://127.0.0.1:"));
    assert!(
        fixture["expires_at_unix"].as_f64().unwrap()
            > time::OffsetDateTime::now_utc().unix_timestamp() as f64 + 120.0
    );
    let identity = Arc::new(IdentityClient::new(endpoint).unwrap());
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    assert!(!identity.refresh().await.unwrap().authenticated);
    login(&identity, &http, mailbox, owner).await;
    let company = OrganizationsClient::new(identity.clone());
    let admin = company.overview().await.unwrap();
    assert!(admin.member.active && admin.member.role == OrganizationRoleView::Admin);
    let remove = company
        .change_member(admin.member.user_id.clone(), admin.member.revision, None)
        .await
        .unwrap();
    assert_eq!(
        company.execute(&remove).await.err().unwrap().code(),
        "last_administrator"
    );
    assert!(identity.view().authenticated);
    let email = format!("{}@example.invalid", uuid::Uuid::now_v7());
    let before = messages(&http, mailbox).await;
    let create = company
        .invite(email.clone(), OrganizationRoleView::User)
        .await
        .unwrap();
    let issued = invitation(company.execute(&create).await.unwrap());
    assert_eq!(
        invitation(company.execute(&create).await.unwrap()).id,
        issued.id
    );
    assert_eq!(
        company.invitations(None).await.unwrap().invitations.len(),
        1
    );
    let message = mail(&http, mailbox, &email, &before, "#invite/").await;
    let fragment = message
        .split_whitespace()
        .find(|word| word.contains("#invite/"))
        .unwrap();
    // The server's configured origin is authoritative; the fixture uses this loopback origin.
    assert!(fragment.starts_with(endpoint));
    let cancel_email = format!("{}@example.invalid", uuid::Uuid::now_v7());
    let cancel = company
        .invite(cancel_email, OrganizationRoleView::User)
        .await
        .unwrap();
    let cancelled = invitation(company.execute(&cancel).await.unwrap());
    let revoke = company.revoke(cancelled.id).await.unwrap();
    assert!(
        invitation(company.execute(&revoke).await.unwrap()).status
            == OrganizationInvitationStatusView::Revoked
    );
    assert!(
        invitation(company.execute(&revoke).await.unwrap()).status
            == OrganizationInvitationStatusView::Revoked
    );
    assert_eq!(
        identity
            .import_invitation(fragment.into())
            .await
            .err()
            .unwrap()
            .code(),
        "authorization_denied"
    );
    identity.logout().await.unwrap();
    assert!(
        identity
            .import_invitation(fragment.into())
            .await
            .unwrap()
            .invitation_pending
    );
    login(&identity, &http, mailbox, &email).await;
    assert!(!identity.view().invitation_pending);
    let user = company.overview().await.unwrap();
    assert!(
        user.company_id == admin.company_id
            && user.member.user_id != admin.member.user_id
            && user.member.role == OrganizationRoleView::User
    );
    assert_eq!(
        company.members(None).await.err().unwrap().code(),
        "authorization_denied"
    );
    assert!(identity.view().authenticated);
    assert!(company.execute(&create).await.is_err()); // old owner's opaque command cannot cross login
    identity.logout().await.unwrap();
    // Honor the real resend budget; do not change the server clock or fixture DB.
    tokio::time::sleep(Duration::from_secs(61)).await;
    login(&identity, &http, mailbox, owner).await;
    let members = company.members(None).await.unwrap();
    let member = members
        .members
        .iter()
        .find(|entry| entry.user_id == user.member.user_id)
        .unwrap();
    let promote = company
        .change_member(
            member.user_id.clone(),
            member.revision,
            Some(OrganizationRoleView::Admin),
        )
        .await
        .unwrap();
    let result = company.execute(&promote).await.unwrap();
    let OrganizationCommandResult::Member(promoted) = result else {
        panic!("expected member");
    };
    assert!(
        promoted.role == OrganizationRoleView::Admin && promoted.revision == member.revision + 1
    );
    let stale = company
        .change_member(member.user_id.clone(), member.revision, None)
        .await
        .unwrap();
    assert_eq!(
        company.execute(&stale).await.err().unwrap().code(),
        "operation_conflict"
    );
    let remove = company
        .change_member(member.user_id.clone(), promoted.revision, None)
        .await
        .unwrap();
    let OrganizationCommandResult::Member(removed) = company.execute(&remove).await.unwrap() else {
        panic!("expected member");
    };
    assert!(!removed.active);
    assert!(identity.view().authenticated);
    identity.logout().await.unwrap();
    println!(
        "Owned Rust company overview, invitation replay/revoke/OTP, owner isolation, roles, revision conflict and last-admin protection passed"
    );
}
