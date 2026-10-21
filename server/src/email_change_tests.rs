//! A user adds or changes their email address on the account page. With
//! mail, a new address is used once a link sent to it is opened, and the
//! old address hears of the change; without mail it is set at once, except
//! where single sign-on joins accounts by email.

use crate::testutil::{add_user, send, test_app};
use axum::http::{Method, StatusCode};
use serde_json::{json, Value};

const PASSWORD: &str = "kontots lösenord";

async fn with_password(db: &sqlx::AnyPool, user: i64) {
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(crate::db::hash_password(PASSWORD).await.unwrap())
        .bind(user)
        .execute(db)
        .await
        .unwrap();
}

fn change(email: &str) -> Value {
    json!({ "username": "alice", "current_password": PASSWORD, "email": email })
}

/// The token in a mailed link, with the quoted-printable wrapping undone.
fn token_in(message: &str) -> String {
    let text = message.replace("=\n", "").replace("=3D", "=");
    text.split("token=").nth(1).unwrap().chars().take_while(|c| c.is_ascii_hexdigit()).collect()
}

#[tokio::test]
async fn a_new_address_is_used_once_the_link_sent_to_it_is_opened() {
    let (_, db, mut state) = test_app().await;
    let (port, sink) = crate::kindle_tests::mail_sink_many(2).await;
    state.mail = Some(crate::mail::MailConfig::local(port, "Legejo <mail@legejo.test>"));
    let app = crate::router(state);
    let (alice, cookie) = add_user(&db, "alice").await;
    with_password(&db, alice).await;
    sqlx::query("UPDATE users SET email = 'alice@old.example' WHERE id = ?").bind(alice).execute(&db).await.unwrap();
    let (bob, _) = add_user(&db, "bob").await;
    sqlx::query("UPDATE users SET email = 'bob@example.org' WHERE id = ?").bind(bob).execute(&db).await.unwrap();

    // Someone else's address, or something that is not one: refused.
    let (status, _) = send(&app, Method::PUT, "/api/account", Some(&cookie), Some(change("BOB@example.org"))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = send(&app, Method::PUT, "/api/account", Some(&cookie), Some(change("no address"))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut wrong = change("alice@new.example");
    wrong["current_password"] = json!("fel");
    let (status, _) = send(&app, Method::PUT, "/api/account", Some(&cookie), Some(wrong)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Asked for: waiting, the old address still in use.
    let (status, v) = send(&app, Method::PUT, "/api/account", Some(&cookie), Some(change("alice@new.example"))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["email"], "alice@old.example");
    assert_eq!(v["pending_email"], "alice@new.example");
    assert_eq!(v["mail_configured"], true);
    let mails = sink.await.unwrap();
    assert_eq!(mails[0].0, "alice@new.example");
    assert_eq!(mails[1].0, "alice@old.example", "the old address hears of it");
    let token = token_in(&mails[0].1);

    // The link, opened without being logged in.
    let (status, v) = send(&app, Method::POST, "/api/account/email/confirm", None, Some(json!({ "token": token }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["email"], "alice@new.example");
    let (_, account) = send(&app, Method::GET, "/api/account", Some(&cookie), None).await;
    assert_eq!(account["email"], "alice@new.example");
    assert!(account["pending_email"].is_null());
    // Once only.
    let (status, _) = send(&app, Method::POST, "/api/account/email/confirm", None, Some(json!({ "token": token }))).await;
    assert_eq!(status, StatusCode::GONE);

    // Removed: at once, nothing to confirm.
    let (status, v) = send(&app, Method::PUT, "/api/account", Some(&cookie), Some(change(""))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert!(v["email"].is_null());
    let (_, log) = {
        sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(alice).execute(&db).await.unwrap();
        send(&app, Method::GET, "/api/admin/log", Some(&cookie), None).await
    };
    let actions: Vec<&str> = log["entries"].as_array().unwrap().iter().filter_map(|e| e["action"].as_str()).collect();
    assert!(actions.contains(&"account.email_confirmed"), "{actions:?}");
}

#[tokio::test]
async fn without_mail_the_address_is_set_at_once_but_not_with_single_sign_on() {
    let (app, db, mut state) = test_app().await;
    let (alice, cookie) = add_user(&db, "alice").await;
    with_password(&db, alice).await;
    let (status, v) = send(&app, Method::PUT, "/api/account", Some(&cookie), Some(change("alice@example.org"))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["email"], "alice@example.org");
    assert_eq!(v["mail_configured"], false);
    // Older clients send no email: it stays.
    let (_, v) = send(&app, Method::PUT, "/api/account", Some(&cookie), Some(json!({ "username": "alice", "current_password": PASSWORD }))).await;
    assert_eq!(v["email"], "alice@example.org");

    // With single sign-on, an address nobody confirmed could join another's sign-in to this account.
    let config = crate::oidc::OidcConfig {
        issuer: "https://id.example".into(),
        client_id: "legejo".into(),
        client_secret: "hemligt".into(),
        name: "Pocket ID".into(),
        auto_create: false,
        trust_email: false,
        admin_group: None,
        scopes: "openid profile email".into(),
    };
    state.oidc = crate::oidc::Oidc::new(Some(config));
    let app = crate::router(state);
    let (status, v) = send(&app, Method::PUT, "/api/account", Some(&cookie), Some(change("other@example.org"))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    let (_, account) = send(&app, Method::GET, "/api/account", Some(&cookie), None).await;
    assert_eq!(account["email"], "alice@example.org");
}
