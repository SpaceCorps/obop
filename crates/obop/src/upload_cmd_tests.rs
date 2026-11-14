//! `obop upload`: a local file becomes an upload of an instance and the value of a record's
//! field, in process on a database and as a client of a served instance, with the refusals of
//! both ways.

use crate::app::App;
use crate::upload_cmd::{Args, Refused, guess_content_type, instance_url, upload};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_media::fixtures as fx;
use serde_json::{Value as J, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;

/// A directory of this test alone, under the temporary directory of the machine.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "obop-upload-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> String {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path.to_str().unwrap().to_string()
}

/// The options of a command on a local database with the fixed dev principals.
fn local_opts(dir: &std::path::Path, token: Option<&str>) -> crate::Opts {
    crate::Opts {
        db: Some(format!("sqlite://{}", dir.join("cli.db").to_str().unwrap())),
        token: token.map(String::from),
        demo: false,
        dev_auth: true,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    }
}

/// The options of a command that talks to a served instance: a token and no database.
fn remote_opts(token: Option<&str>) -> crate::Opts {
    crate::Opts {
        db: None,
        token: token.map(String::from),
        demo: false,
        dev_auth: false,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    }
}

fn args<'a>(file: &'a str, field: &'a str) -> Args<'a> {
    Args {
        file,
        field: Some(field),
        record: None,
        content_type: None,
        url: None,
    }
}

fn line(r: Result<J, Refused>) -> String {
    match r {
        Ok(v) => panic!("the upload was accepted: {v}"),
        Err(e) => e.line().to_string(),
    }
}

fn accepted(r: Result<J, Refused>) -> J {
    match r {
        Ok(v) => v,
        Err(e) => panic!("the upload was refused: {}", e.line()),
    }
}

fn new_product(o: &crate::Opts) -> String {
    let (v, _) = crate::call_json(o, "product", "create", &["name=Teapot"]).unwrap();
    v["id"].as_str().unwrap().to_string()
}

fn image_of(o: &crate::Opts, id: &str) -> J {
    let (v, _) = crate::call_json(o, "product", "get", &[&format!("id={id}")]).unwrap();
    v["image"].clone()
}

#[test]
fn a_local_upload_is_stored_and_becomes_the_field_of_the_record() {
    let dir = scratch("local");
    let o = local_opts(&dir, None);
    let png = fx::png(8, 8, 1);
    let file = write(&dir, "shot.png", &png);
    let id = new_product(&o);

    // Without --record the file is stored and nothing else changes.
    let alone = accepted(upload(&o, &args(&file, "product.image")));
    assert_eq!(alone["content_type"], "image/png");
    assert_eq!(alone["filename"], "shot.png");
    assert_eq!(alone["sha256"].as_str().unwrap().len(), 64);
    assert!(alone["size"].as_u64().unwrap() > 0);
    assert!(alone.get("record").is_none());
    assert_eq!(image_of(&o, &id), J::Null);

    // With --record the field of that record is the upload.
    let attached = accepted(upload(
        &o,
        &Args {
            record: Some(&id),
            ..args(&file, "product.image")
        },
    ));
    assert_eq!(
        attached["record"],
        json!({"model": "product", "id": id, "field": "image"})
    );
    assert_eq!(image_of(&o, &id), attached["id"]);
    assert_eq!(
        attached["sha256"], alone["sha256"],
        "the hash is of the local file"
    );
}

#[test]
fn a_local_upload_is_refused_like_the_rest_route_refuses_it() {
    let dir = scratch("refused");
    let o = local_opts(&dir, None);
    let png = write(&dir, "shot.png", &fx::png(8, 8, 2));
    let text = write(&dir, "notes.png", b"not a picture at all");

    // The command line itself.
    let no_field = upload(
        &o,
        &Args {
            field: None,
            ..args(&png, "")
        },
    );
    assert!(matches!(no_field, Err(Refused::Line(l, 2)) if l.starts_with("usage: obop upload")));
    for bad in ["product", "product.image.x", "Product.image", "../x.y", "."] {
        let l = line(upload(&o, &args(&png, bad)));
        assert!(l.contains("model.field"), "{bad}: {l}");
    }
    let missing = dir.join("nothing.png");
    assert!(
        line(upload(
            &o,
            &args(missing.to_str().unwrap(), "product.image")
        ))
        .starts_with("cannot read ")
    );
    let blank = upload(
        &o,
        &Args {
            record: Some(" "),
            ..args(&png, "product.image")
        },
    );
    assert!(line(blank).contains("--record needs the id"));

    // The rules of the upload: an unknown field, a field that takes no file, bytes that are not
    // what the name says.
    for not_a_blob_field in ["nope.image", "product.name"] {
        let l = line(upload(&o, &args(&png, not_a_blob_field)));
        assert!(l.starts_with("error: "), "{not_a_blob_field}: {l}");
    }
    let l = line(upload(&o, &args(&text, "product.image")));
    assert!(l.starts_with("error: "), "{l}");

    // The groups of the caller: dev-sales reads products and may not write them.
    let sales = local_opts(&dir, Some("dev-sales"));
    assert_eq!(
        line(upload(&sales, &args(&png, "product.image"))),
        "error: you may not upload to this field"
    );

    // A record that is not there: the line says the file is stored and the record is not changed.
    let l = line(upload(
        &o,
        &Args {
            record: Some("01900000-0000-7000-8000-000000000000"),
            ..args(&png, "product.image")
        },
    ));
    assert!(
        l.contains("the file is uploaded as ") && l.contains("the record was not changed"),
        "{l}"
    );
}

#[test]
fn the_type_is_taken_from_the_file_name() {
    assert_eq!(guess_content_type("Contract.PDF"), "application/pdf");
    assert_eq!(guess_content_type("a.b.jpeg"), "image/jpeg");
    assert_eq!(
        guess_content_type("offer.docx"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    );
    assert_eq!(guess_content_type("README"), "");
    assert_eq!(guess_content_type("page.html"), "");
    // Every type the guess can say is one an upload accepts.
    for name in [
        "a.pdf", "a.png", "a.jpg", "a.gif", "a.webp", "a.txt", "a.csv", "a.zip", "a.docx",
        "a.xlsx", "a.pptx", "a.odt", "a.ods", "a.odp",
    ] {
        assert!(
            obop_media::FILE_TYPES.contains(&guess_content_type(name)),
            "{name}"
        );
    }
}

#[test]
fn the_token_goes_to_https_or_to_this_machine_only() {
    assert_eq!(
        instance_url("https://shop.example.test/").unwrap(),
        "https://shop.example.test"
    );
    assert_eq!(
        instance_url(" http://127.0.0.1:8080 ").unwrap(),
        "http://127.0.0.1:8080"
    );
    assert!(instance_url("http://localhost:8080").is_ok());
    assert!(instance_url("http://[::1]:8080").is_ok());
    for refused in [
        "http://shop.example.test",
        "http://127.0.0.1.example.test",
        "http://localhost.example.test:8080",
        "http://localhost:80@shop.example.test",
        "http://[::1]shop.example.test",
        "shop.example.test",
        "ftp://shop.example.test",
        "https://",
        "https://user@shop.example.test",
        "https://shop.example.test/?next=x",
        "https://shop.example.test/#x",
    ] {
        assert!(instance_url(refused).is_err(), "{refused}");
    }
    // Refused before the file or the token goes anywhere.
    let dir = scratch("plain");
    let png = write(&dir, "shot.png", &fx::png(8, 8, 3));
    let l = line(upload(
        &remote_opts(Some("a-token")),
        &Args {
            url: Some("http://shop.example.test"),
            ..args(&png, "product.image")
        },
    ));
    assert!(l.contains("not sent over plain http"), "{l}");
}

/// The router served on a loopback socket, and its address as `--url` takes it.
async fn on_a_socket(router: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}")
}

async fn json_call(r: &axum::Router, token: Option<&str>, path: &str, body: J) -> (u16, J) {
    let mut b = Request::post(path).header("content-type", "application/json");
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let resp = r
        .clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(J::Null))
}

/// An instance with one person who may write products, and that person's token.
async fn instance() -> (axum::Router, String, App) {
    let app = App::new();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    let groups: Vec<String> = ["product.user", "product.manager", "base.user"]
        .iter()
        .map(|g| g.to_string())
        .collect();
    auth.create_user("maker", "a long enough password", &groups)
        .unwrap();
    let router = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth),
        None,
    );
    let (s, v) = json_call(
        &router,
        None,
        "/api/auth/login",
        json!({"username": "maker", "password": "a long enough password"}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let token = v["token"].as_str().unwrap().to_string();
    (router, token, app)
}

/// The command as the binary runs it: on a thread that may block.
async fn run_upload(o: crate::Opts, a: OwnedArgs) -> Result<J, String> {
    tokio::task::spawn_blocking(move || {
        upload(
            &o,
            &Args {
                file: &a.file,
                field: Some(&a.field),
                record: a.record.as_deref(),
                content_type: None,
                url: Some(&a.url),
            },
        )
        .map_err(|e| e.line().to_string())
    })
    .await
    .unwrap()
}

struct OwnedArgs {
    file: String,
    field: String,
    record: Option<String>,
    url: String,
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_to_a_served_instance_is_stored_and_attached() {
    let (router, token, _app) = instance().await;
    let url = on_a_socket(router.clone()).await;
    let dir = scratch("remote");
    let file = write(&dir, "shot.png", &fx::png(8, 8, 4));
    let (s, v) = json_call(
        &router,
        Some(&token),
        "/api/product/create",
        json!({"name": "Teapot"}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let id = v["value"]["id"].as_str().unwrap().to_string();
    let call = |record: Option<&str>, token: Option<&str>, file: &str, field: &str| {
        run_upload(
            remote_opts(token),
            OwnedArgs {
                file: file.to_string(),
                field: field.to_string(),
                record: record.map(String::from),
                url: url.clone(),
            },
        )
    };

    let answer = call(Some(&id), Some(&token), &file, "product.image")
        .await
        .unwrap();
    assert_eq!(answer["content_type"], "image/png");
    assert_eq!(
        answer["record"],
        json!({"model": "product", "id": id, "field": "image"})
    );
    let (s, v) = json_call(&router, Some(&token), "/api/product/get", json!({"id": id})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["value"]["image"], answer["id"]);

    // The refusals of the instance reach the command line in its words.
    let l = call(None, Some("not-a-token"), &file, "product.image")
        .await
        .unwrap_err();
    assert_eq!(l, "error: the instance did not accept the token (401)");
    let l = call(None, None, &file, "product.image").await.unwrap_err();
    assert!(l.starts_with("no token: pass --token"), "{l}");
    let l = call(None, Some(&token), &file, "nope.image")
        .await
        .unwrap_err();
    assert!(l.ends_with("(400)"), "{l}");
    let text = write(&dir, "notes.png", b"not a picture at all");
    let l = call(None, Some(&token), &text, "product.image")
        .await
        .unwrap_err();
    assert!(l.ends_with("(415)"), "{l}");
    let l = call(
        Some("01900000-0000-7000-8000-000000000000"),
        Some(&token),
        &file,
        "product.image",
    )
    .await
    .unwrap_err();
    assert!(
        l.contains("the file is uploaded as ") && l.contains("the record was not changed"),
        "{l}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_redirect_is_not_followed_with_the_token_and_the_file() {
    // The address the redirect points at counts what reaches it.
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = hits.clone();
    let elsewhere = on_a_socket(axum::Router::new().fallback(move || {
        let seen = seen.clone();
        async move {
            seen.fetch_add(1, Ordering::SeqCst);
            axum::http::StatusCode::CREATED
        }
    }))
    .await;
    let target = format!("{elsewhere}/api/blobs");
    let redirecting = on_a_socket(axum::Router::new().fallback(move || {
        let target = target.clone();
        async move {
            (
                axum::http::StatusCode::TEMPORARY_REDIRECT,
                [(axum::http::header::LOCATION, target)],
            )
        }
    }))
    .await;
    let dir = scratch("redirect");
    let file = write(&dir, "shot.png", &fx::png(8, 8, 5));
    let l = run_upload(
        remote_opts(Some("a-token")),
        OwnedArgs {
            file,
            field: "product.image".into(),
            record: None,
            url: redirecting,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(l, "error: the instance answered 307");
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}
