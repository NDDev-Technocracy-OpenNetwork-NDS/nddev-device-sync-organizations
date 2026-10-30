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
/// Own one bounded PostgreSQL lock holder. Both normal and unwinding paths
/// release the transaction and join the native timeout child.
#[cfg(target_os = "linux")]
struct MembershipLock(Option<std::process::Child>);
#[cfg(target_os = "linux")]
impl MembershipLock {
    fn new(container: &str) -> Self {
        use std::io::Write;
        let child = std::process::Command::new("timeout")
            .args([
                "--kill-after=1s",
                "10s",
                "docker",
                "exec",
                "-i",
                container,
                "psql",
                "-XAt",
                "-v",
                "ON_ERROR_STOP=1",
                "-U",
                "postgres",
                "-d",
                "nds",
                "-f",
                "-",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut owner = Self(Some(child));
        owner.0.as_mut().unwrap().stdin.as_mut().unwrap().write_all(b"SET statement_timeout='8s'; SET idle_in_transaction_session_timeout='6s'; BEGIN; LOCK TABLE nds_company_memberships IN ACCESS EXCLUSIVE MODE;\n").unwrap();
        owner
    }
    fn finish(mut self) -> std::io::Result<std::process::ExitStatus> {
        use std::io::Write;
        let mut child = self.0.take().unwrap();
        let sent = child.stdin.as_mut().unwrap().write_all(b"COMMIT;\n");
        drop(child.stdin.take());
        let status = child.wait(); // join even if committing failed
        sent?;
        status
    }
}
#[cfg(target_os = "linux")]
impl Drop for MembershipLock {
    fn drop(&mut self) {
        use std::io::Write;
        if let Some(mut child) = self.0.take() {
            if let Some(mut input) = child.stdin.take() {
                let _ = input.write_all(b"ROLLBACK;\n");
            }
            let _ = child.wait();
        }
    }
}
#[cfg(target_os = "linux")]
fn fixture_count(container: &str, query: &str) -> Option<usize> {
    let output = std::process::Command::new("timeout")
        .args([
            "--kill-after=1s",
            "5s",
            "docker",
            "exec",
            container,
            "psql",
            "-XAt",
            "-v",
            "ON_ERROR_STOP=1",
            "-U",
            "postgres",
            "-d",
            "nds",
            "-c",
            query,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}
#[cfg(target_os = "linux")]
async fn wait_for_fixture_lock(container: &str, query: &str) -> bool {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match fixture_count(container, query) {
                Some(0) => tokio::time::sleep(Duration::from_millis(25)).await,
                Some(_) => return true,
                None => return false,
            }
        }
    })
    .await
    .unwrap_or(false)
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
    let collection = OrganizationsClient::new(identity.clone());
    let choices = collection.list().await.unwrap();
    assert_eq!(choices.len(), 1);
    let company = collection
        .select(choices[0].company_id.clone(), choices[0].member.revision)
        .await
        .unwrap();
    let admin = company.overview().await.unwrap();
    let original_personal = identity.view();
    let new = collection
        .create("Second owned organization".into())
        .await
        .unwrap();
    let OrganizationCommandResult::Organization(second) = new.execute().await.unwrap() else {
        panic!("expected organization");
    };
    let OrganizationCommandResult::Organization(replayed) = new.execute().await.unwrap() else {
        panic!("expected organization");
    };
    assert_eq!(second.company_id, replayed.company_id);
    assert_ne!(second.company_id, original_personal.tenant_id);
    let second_company = collection
        .select(second.company_id.clone(), second.member.revision)
        .await
        .unwrap();
    assert_eq!(collection.list().await.unwrap().len(), 2);
    assert_eq!(identity.view().user_id, original_personal.user_id);
    assert_eq!(identity.view().tenant_id, original_personal.tenant_id);
    assert!(admin.member.active && admin.member.role == OrganizationRoleView::Admin);
    let remove = company
        .change_member(admin.member.user_id.clone(), admin.member.revision, None)
        .await
        .unwrap();
    assert_eq!(
        remove.execute().await.err().unwrap().code(),
        "last_administrator"
    );
    assert!(identity.view().authenticated);
    let email = format!("{}@example.invalid", uuid::Uuid::now_v7());
    let before = messages(&http, mailbox).await;
    let create = company
        .invite(email.clone(), OrganizationRoleView::User)
        .await
        .unwrap();
    let issued = invitation(create.execute().await.unwrap());
    assert_eq!(invitation(create.execute().await.unwrap()).id, issued.id);
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
    let cancelled = invitation(cancel.execute().await.unwrap());
    let revoke = company.revoke(cancelled.id).await.unwrap();
    assert!(
        invitation(revoke.execute().await.unwrap()).status
            == OrganizationInvitationStatusView::Revoked
    );
    assert!(
        invitation(revoke.execute().await.unwrap()).status
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
    let prior_second = messages(&http, mailbox).await;
    let invite_second = second_company
        .invite(email.clone(), OrganizationRoleView::Admin)
        .await
        .unwrap();
    invite_second.execute().await.unwrap();
    let second_mail = mail(&http, mailbox, &email, &prior_second, "#invite/").await;
    let second_link = second_mail
        .split_whitespace()
        .find(|word| word.contains("#invite/"))
        .unwrap();
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
    let user_collection = OrganizationsClient::new(identity.clone());
    let first_choices = user_collection.list().await.unwrap();
    assert_eq!(first_choices.len(), 1);
    let user_company = user_collection
        .select(admin.company_id.clone(), first_choices[0].member.revision)
        .await
        .unwrap();
    let user = user_company.overview().await.unwrap();
    let retained_personal = identity.view();
    assert!(
        user.company_id == admin.company_id
            && user.member.user_id != admin.member.user_id
            && user.member.role == OrganizationRoleView::User
    );
    assert_eq!(
        user_company.members(None).await.err().unwrap().code(),
        "authorization_denied"
    );
    assert!(identity.view().authenticated);
    assert!(create.execute().await.is_err()); // old owner's opaque command cannot cross login
    identity.logout().await.unwrap();
    tokio::time::sleep(Duration::from_secs(61)).await;
    identity
        .import_invitation(second_link.into())
        .await
        .unwrap();
    login(&identity, &http, mailbox, &email).await;
    assert_eq!(identity.view().user_id, retained_personal.user_id);
    assert_eq!(identity.view().tenant_id, retained_personal.tenant_id);
    assert_eq!(
        identity.view().current_device_id,
        retained_personal.current_device_id
    );
    let joined = OrganizationsClient::new(identity.clone());
    let joined_choices = joined.list().await.unwrap();
    assert_eq!(joined_choices.len(), 2);
    let first_choice = joined_choices
        .iter()
        .find(|row| row.company_id == admin.company_id)
        .unwrap();
    let second_choice = joined_choices
        .iter()
        .find(|row| row.company_id == second.company_id)
        .unwrap();
    assert!(first_choice.member.role == OrganizationRoleView::User);
    assert!(second_choice.member.role == OrganizationRoleView::Admin);
    let first_user = joined
        .select(
            first_choice.company_id.clone(),
            first_choice.member.revision,
        )
        .await
        .unwrap();
    let second_admin = joined
        .select(
            second_choice.company_id.clone(),
            second_choice.member.revision,
        )
        .await
        .unwrap();
    assert_eq!(
        first_user.members(None).await.err().unwrap().code(),
        "authorization_denied"
    );
    assert!(second_admin.members(None).await.is_ok());
    assert!(identity.view().authenticated);
    second_admin.close();
    assert_eq!(
        second_admin.overview().await.err().unwrap(),
        nddev_device_sync_accounts::ClientError::RevalidationRequired
    );
    assert!(joined.list().await.is_ok());
    identity.logout().await.unwrap();
    // Honor the real resend budget; do not change the server clock or fixture DB.
    tokio::time::sleep(Duration::from_secs(61)).await;
    login(&identity, &http, mailbox, owner).await;
    let collection = OrganizationsClient::new(identity.clone());
    let choices = collection.list().await.unwrap();
    let choice = choices
        .iter()
        .find(|row| row.company_id == admin.company_id)
        .unwrap();
    let company = collection
        .select(choice.company_id.clone(), choice.member.revision)
        .await
        .unwrap();
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
    let result = promote.execute().await.unwrap();
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
        stale.execute().await.err().unwrap().code(),
        "operation_conflict"
    );
    let remove = company
        .change_member(member.user_id.clone(), promoted.revision, None)
        .await
        .unwrap();
    let OrganizationCommandResult::Member(removed) = remove.execute().await.unwrap() else {
        panic!("expected member");
    };
    assert!(!removed.active);
    assert!(identity.view().authenticated);
    // Traverse the actual SQL keyset boundary; do not assume its collation is
    // Rust byte ordering or synthesize a canned second page.
    for _ in 0..51 {
        let command = company
            .invite(
                format!("{}@example.invalid", uuid::Uuid::now_v7()),
                OrganizationRoleView::User,
            )
            .await
            .unwrap();
        command.execute().await.unwrap();
    }
    let first = company.invitations(None).await.unwrap();
    assert_eq!(first.invitations.len(), 50);
    let second = company.invitations(first.next_cursor).await.unwrap();
    assert_eq!(second.invitations.len(), 3);
    assert!(second.next_cursor.is_none());
    assert!(
        second
            .invitations
            .iter()
            .all(|item| !first.invitations.iter().any(|old| old.id == item.id))
    );
    #[cfg(target_os = "linux")]
    {
        let container = fixture["database_container"].as_str().unwrap();
        assert!(
            container.starts_with("nds-db-check-")
                && container[13..].bytes().all(|b| b.is_ascii_alphanumeric())
        );
        let effect_email = format!("held-{}@example.invalid", uuid::Uuid::now_v7());
        let command = Arc::new(
            company
                .invite(effect_email.clone(), OrganizationRoleView::User)
                .await
                .unwrap(),
        );
        let hold = MembershipLock::new(container);
        assert!(wait_for_fixture_lock(container,"SELECT count(*) FROM pg_locks WHERE relation='nds_company_memberships'::regclass AND mode='AccessExclusiveLock' AND granted").await);
        let execution = tokio::spawn({
            let command = command.clone();
            async move { command.execute().await }
        });
        let waited=wait_for_fixture_lock(container,"SELECT count(*) FROM pg_locks l JOIN pg_stat_activity a USING(pid) WHERE l.relation='nds_company_memberships'::regclass AND NOT l.granted AND a.usename='nds_runtime' AND a.query LIKE 'SELECT o.user_id,o.tenant_id,m.tenant_id AS company_id%'").await;
        company.close();
        let released = hold.finish();
        let result = execution.await; // join admitted work before any result assertion
        assert!(waited);
        assert!(released.unwrap().success());
        assert_eq!(
            result.unwrap().err().unwrap(),
            nddev_device_sync_accounts::ClientError::RevalidationRequired
        );
        let replacement = collection
            .select(admin.company_id.clone(), admin.member.revision)
            .await
            .unwrap();
        company.close(); // late old close cannot close the replacement
        assert!(replacement.check_publication().is_ok());
        let observed = replacement.invitations(None).await.unwrap();
        // Traverse actual pages: this effect may sort after the first50.
        let mut page = observed;
        let mut found = false;
        for _ in 0..4 {
            found |= page.invitations.iter().any(|row| row.email == effect_email);
            let Some(after) = page.next_cursor else {
                break;
            };
            page = replacement.invitations(Some(after)).await.unwrap();
        }
        assert!(found, "closed caller falsely implied no admitted effect");
        assert!(identity.view().authenticated);
    }
    identity.logout().await.unwrap();
    println!(
        "Owned Rust personal+two organizations, create replay, mixed roles, same original owner/device on join, closed/old scopes, invitation OTP, revision conflict and last-admin protection passed"
    );
}
