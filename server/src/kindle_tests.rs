//! Send to Kindle: the address on the account and the mail with the book.

use crate::testutil::{add_book, add_user, send, test_app};
use axum::http::{Method, StatusCode};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// A mail server that accepts one message and hands back what it was sent:
/// the recipient and the message itself.
async fn mail_sink() -> (u16, tokio::task::JoinHandle<(String, String)>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        write.write_all(b"220 sink\r\n").await.unwrap();
        let (mut to, mut message, mut in_data) = (String::new(), String::new(), false);
        while let Some(line) = lines.next_line().await.unwrap() {
            if in_data {
                if line == "." {
                    in_data = false;
                    write.write_all(b"250 ok\r\n").await.unwrap();
                } else {
                    message.push_str(&line);
                    message.push('\n');
                }
                continue;
            }
            let command = line.to_uppercase();
            let reply: &[u8] = if command.starts_with("DATA") {
                in_data = true;
                b"354 go on\r\n"
            } else if command.starts_with("QUIT") {
                write.write_all(b"221 bye\r\n").await.unwrap();
                break;
            } else {
                if command.starts_with("RCPT TO:") {
                    to = line[8..].trim().trim_matches(['<', '>']).to_string();
                }
                b"250 ok\r\n"
            };
            write.write_all(reply).await.unwrap();
        }
        (to, message)
    });
    (port, task)
}

#[tokio::test]
async fn a_book_is_mailed_to_the_kindle() {
    let (_, db, mut state) = test_app().await;
    let (port, sink) = mail_sink().await;
    state.mail = Some(crate::mail::MailConfig::local(port, "Legejo <books@legejo.test>"));
    let app = crate::router(state.clone());
    let (alice, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    let (book, uuid) = add_book(&db, alice, "Röda rummet: en \"skildring\"").await;
    std::fs::write(state.data_dir.join("books").join(format!("{uuid}.epub")), b"PK\x03\x04\xff\xfe").unwrap();
    let send_uri = format!("/api/books/{book}/kindle");

    // Nothing to send to yet, and the button stays away.
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&a), None).await;
    assert_eq!(config["send_to_kindle"], false);
    let (status, v) = send(&app, Method::POST, &send_uri, Some(&a), None).await;
    assert_eq!((status, v["error"].as_str()), (StatusCode::CONFLICT, Some("no-address")));

    // Only Amazon's addresses are taken.
    for bad in ["someone@example.org", "kindle.com", "x@notkindle.com.example.org"] {
        let (status, v) = send(&app, Method::PUT, "/api/account/kindle", Some(&a), Some(json!({ "email": bad }))).await;
        assert_eq!((status, v["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("not-kindle")), "{bad}");
    }
    let (status, account) =
        send(&app, Method::PUT, "/api/account/kindle", Some(&a), Some(json!({ "email": " Alice_1@Kindle.com " }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(account["kindle_email"], "Alice_1@Kindle.com");
    assert_eq!(account["mail_from"], "books@legejo.test");
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&a), None).await;
    assert_eq!(config["send_to_kindle"], true);

    // Someone else's book is not theirs to send.
    send(&app, Method::PUT, "/api/account/kindle", Some(&b), Some(json!({ "email": "bob@kindle.com" }))).await;
    assert_eq!(send(&app, Method::POST, &send_uri, Some(&b), None).await.0, StatusCode::NOT_FOUND);

    let (status, _) = send(&app, Method::POST, &send_uri, Some(&a), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (to, message) = sink.await.unwrap();
    assert_eq!(to, "Alice_1@Kindle.com");
    assert!(message.contains("application/epub+zip"), "{message}");
    assert!(message.contains("filename=\"R_da rummet_ en _skildring_.epub\""), "{message}");
    // The file's bytes, base64-encoded.
    assert!(message.contains("UEsDBP/+"), "{message}");
    let logged: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log WHERE action = 'book.sent_to_kindle'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(logged, 1);

    // The address can be taken away again.
    let (_, account) = send(&app, Method::PUT, "/api/account/kindle", Some(&a), Some(json!({ "email": "" }))).await;
    assert!(account["kindle_email"].is_null());
}

#[tokio::test]
async fn without_mail_nothing_is_sent() {
    let (app, db, state) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (book, uuid) = add_book(&db, alice, "Bok").await;
    std::fs::write(state.data_dir.join("books").join(format!("{uuid}.epub")), "x").unwrap();
    send(&app, Method::PUT, "/api/account/kindle", Some(&a), Some(json!({ "email": "a@kindle.com" }))).await;
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&a), None).await;
    assert_eq!(config["send_to_kindle"], false);
    let (status, v) = send(&app, Method::POST, &format!("/api/books/{book}/kindle"), Some(&a), None).await;
    assert_eq!((status, v["error"].as_str()), (StatusCode::CONFLICT, Some("no-mail")));
}
