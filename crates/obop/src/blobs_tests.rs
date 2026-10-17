//! Images and files over HTTP (D60): upload, reference, download, thumbnails and the ways to
//! abuse them, against the real router, authenticator and bundled modules.

use crate::app::App;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_media::fixtures as fx;
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

struct Fx {
    router: axum::Router,
    app: App,
    admin: String,
    /// Reads products (product.user) but cannot write them.
    reader: String,
    /// Holds a group that has nothing to do with products.
    outsider: String,
    agent: String,
}

async fn send(r: &axum::Router, req: Request<Body>) -> (u16, Vec<u8>, axum::http::HeaderMap) {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, bytes.to_vec(), headers)
}

async fn json_call(r: &axum::Router, token: Option<&str>, path: &str, body: J) -> (u16, J) {
    let mut b = Request::post(path).header("content-type", "application/json");
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let (s, bytes, _) = send(r, b.body(Body::from(body.to_string())).unwrap()).await;
    (s, serde_json::from_slice(&bytes).unwrap_or(J::Null))
}

async fn fixture() -> Fx {
    let app = App::new();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    let pw = "a long enough password";
    let all: Vec<String> = [
        "platform.admin",
        "product.user",
        "product.manager",
        "base.user",
        "privacy.officer",
    ]
    .iter()
    .map(|g| g.to_string())
    .collect();
    auth.create_user("admin", pw, &all).unwrap();
    auth.create_user("reader", pw, &["product.user".into()])
        .unwrap();
    auth.create_user("outsider", pw, &["crm.user".into()])
        .unwrap();
    let router = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth),
        None,
    );
    let login = |u: &'static str| {
        let r = router.clone();
        async move {
            let (s, v) = json_call(
                &r,
                None,
                "/api/auth/login",
                json!({"username": u, "password": "a long enough password"}),
            )
            .await;
            assert_eq!(s, 200);
            v["token"].as_str().unwrap().to_string()
        }
    };
    let (admin, reader, outsider) = (
        login("admin").await,
        login("reader").await,
        login("outsider").await,
    );
    let (s, v) = json_call(
        &router,
        Some(&admin),
        "/api/auth/agent-token",
        json!({"scopes": ["product.user", "product.manager"], "label": "bot"}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let agent = v["token"].as_str().unwrap().to_string();
    Fx {
        router,
        app,
        admin,
        reader,
        outsider,
        agent,
    }
}

async fn upload(
    f: &Fx,
    token: Option<&str>,
    field: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> (u16, J, axum::http::HeaderMap) {
    let mut b = Request::post(format!("/api/blobs?field={field}&filename=shot.png"))
        .header("content-type", content_type);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let (s, body, h) = send(&f.router, b.body(Body::from(bytes)).unwrap()).await;
    (s, serde_json::from_slice(&body).unwrap_or(J::Null), h)
}

async fn download(
    f: &Fx,
    token: Option<&str>,
    path: &str,
) -> (u16, Vec<u8>, axum::http::HeaderMap) {
    let mut b = Request::get(path);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    send(&f.router, b.body(Body::empty()).unwrap()).await
}

async fn create_product(f: &Fx, image: &str) -> String {
    let (s, v) = json_call(
        &f.router,
        Some(&f.admin),
        "/api/product/create",
        json!({"name": "Teapot", "image": image}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    v["value"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn upload_attach_download_and_thumbnail_over_http() {
    let f = fixture().await;
    let (s, v, _) = upload(
        &f,
        Some(&f.admin),
        "product.image",
        "image/png",
        fx::png(1400, 700, 3),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(1400), Some(700))
    );
    assert_eq!(v["content_type"], "image/png");
    assert_eq!(id.len(), 64);
    // Saved into a record, the id comes back in every read and the blob is held.
    let rec = create_product(&f, &id).await;
    let (s, v) = json_call(
        &f.router,
        Some(&f.admin),
        "/api/product/get",
        json!({"id": rec}),
    )
    .await;
    assert_eq!((s, v["value"]["image"].as_str()), (200, Some(id.as_str())));
    assert_eq!(
        f.app.store.blobs().info(&id).unwrap().unwrap().owners.len(),
        1
    );
    // Download: right type, long private cache, hash validator, nosniff, sandboxed.
    let (s, body, h) = download(&f, Some(&f.admin), &format!("/api/blobs/{id}")).await;
    assert_eq!(s, 200);
    assert_eq!(h["content-type"], "image/png");
    assert!(h["cache-control"].to_str().unwrap().contains("private"));
    assert!(h["cache-control"].to_str().unwrap().contains("immutable"));
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert!(
        h["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    assert!(
        h["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline")
    );
    assert_eq!(h["etag"], format!("\"{id}\""));
    assert_eq!(h["vary"], "Authorization");
    assert!(body.starts_with(&[0x89, b'P', b'N', b'G']));
    // Revalidation.
    let r = Request::get(format!("/api/blobs/{id}"))
        .header("authorization", format!("Bearer {}", f.admin))
        .header("if-none-match", format!("\"{id}\""))
        .body(Body::empty())
        .unwrap();
    let (s, body, _) = send(&f.router, r).await;
    assert_eq!((s, body.len()), (304, 0));
    // Thumbnails: fixed widths only.
    let (s, small, h) = download(&f, Some(&f.admin), &format!("/api/blobs/{id}?w=320")).await;
    assert_eq!(s, 200);
    assert!(small.len() < 20_000 && h["etag"].to_str().unwrap().contains("-w320"));
    assert_eq!(
        download(&f, Some(&f.admin), &format!("/api/blobs/{id}?w=321"))
            .await
            .0,
        404
    );
    // Someone who can read products can read the image; a stranger cannot.
    assert_eq!(
        download(&f, Some(&f.reader), &format!("/api/blobs/{id}"))
            .await
            .0,
        200
    );
    assert_eq!(
        download(&f, Some(&f.outsider), &format!("/api/blobs/{id}"))
            .await
            .0,
        404
    );
    // Not signed in.
    assert_eq!(download(&f, None, &format!("/api/blobs/{id}")).await.0, 401);
    assert_eq!(
        download(&f, Some("nonsense"), &format!("/api/blobs/{id}"))
            .await
            .0,
        401
    );
    // An agent holds the same groups and still gets no bytes, only the id from the record.
    assert_eq!(
        download(&f, Some(&f.agent), &format!("/api/blobs/{id}"))
            .await
            .0,
        404
    );
    let (s, v) = json_call(
        &f.router,
        Some(&f.agent),
        "/api/product/get",
        json!({"id": rec}),
    )
    .await;
    assert_eq!((s, v["value"]["image"].as_str()), (200, Some(id.as_str())));
    // Deleting the record releases the blob.
    let (s, _) = json_call(
        &f.router,
        Some(&f.admin),
        "/api/product/delete",
        json!({"id": rec}),
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(
        download(&f, Some(&f.admin), &format!("/api/blobs/{id}"))
            .await
            .0,
        404
    );
}

#[tokio::test]
async fn uploads_need_a_token_a_writable_image_field_and_honest_content() {
    let f = fixture().await;
    let png = fx::png(32, 32, 1);
    assert_eq!(
        upload(&f, None, "product.image", "image/png", png.clone())
            .await
            .0,
        401
    );
    assert_eq!(
        upload(&f, Some("bad"), "product.image", "image/png", png.clone())
            .await
            .0,
        401
    );
    // A reader may not write products; an outsider does not even see the model.
    assert_eq!(
        upload(
            &f,
            Some(&f.reader),
            "product.image",
            "image/png",
            png.clone()
        )
        .await
        .0,
        403
    );
    assert_eq!(
        upload(
            &f,
            Some(&f.outsider),
            "product.image",
            "image/png",
            png.clone()
        )
        .await
        .0,
        403
    );
    // The target must be a real image or file field.
    for field in [
        "",
        "product",
        "product.name",
        "product.nope",
        "nope.image",
        "../x.y",
        "product.image.x",
    ] {
        let (s, _, _) = upload(&f, Some(&f.admin), field, "image/png", png.clone()).await;
        assert_eq!(s, 400, "{field:?}");
    }
    // Content that is not what it says, or not an image at all.
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("image/svg+xml", fx::SVG_WITH_SCRIPT.as_bytes().to_vec()),
        ("image/png", fx::SVG_WITH_SCRIPT.as_bytes().to_vec()),
        ("image/jpeg", fx::png(8, 8, 1)),
        (
            "image/png",
            b"<html><script>alert(1)</script></html>".to_vec(),
        ),
        ("image/gif", b"GIF89a<script>alert(1)</script>".to_vec()),
        ("image/png", fx::png_bomb(60_000, 60_000)),
        ("image/png", vec![]),
        ("image/png", vec![0u8; 100]),
        ("text/html", b"<h1>hi</h1>".to_vec()),
    ];
    for (ct, body) in cases {
        let (s, _, _) = upload(&f, Some(&f.admin), "product.image", ct, body).await;
        assert!(s == 415 || s == 400, "{ct}: {s}");
    }
    // The size cap of the field: by Content-Length, and by the bytes actually sent.
    let r = Request::post("/api/blobs?field=base.photo".replace("base", "party"))
        .header("authorization", format!("Bearer {}", f.admin))
        .header("content-length", "99999999")
        .body(Body::from(png.clone()))
        .unwrap();
    assert_eq!(send(&f.router, r).await.0, 413);
    let big = vec![0xFFu8; 6 * 1024 * 1024];
    assert_eq!(
        upload(&f, Some(&f.admin), "product.image", "image/png", big)
            .await
            .0,
        413
    );
    // Metadata is gone from what was stored, and the stored copy is still a valid picture.
    let (s, v, _) = upload(
        &f,
        Some(&f.admin),
        "product.image",
        "image/jpeg",
        fx::jpeg_with_gps(40, 40, 2),
    )
    .await;
    assert_eq!(s, 201);
    let id = v["id"].as_str().unwrap();
    let (_, body, h) = download(&f, Some(&f.admin), &format!("/api/blobs/{id}")).await;
    assert_eq!(h["content-type"], "image/jpeg");
    assert!(
        !body
            .windows(fx::GPS_MARKER.len())
            .any(|w| w == fx::GPS_MARKER.as_bytes())
    );
    // The contact photo needs base.user, which the outsider lacks.
    let (s, v, _) = upload(
        &f,
        Some(&f.outsider),
        "party.photo",
        "image/png",
        fx::png(16, 16, 1),
    )
    .await;
    assert_eq!(s, 403, "{v}");
}

#[tokio::test]
async fn ids_are_hashes_never_paths_and_other_peoples_uploads_are_not_usable() {
    let f = fixture().await;
    for bad in [
        "..%2F..%2Fetc%2Fpasswd",
        "%2e%2e%2f%2e%2e%2fCargo.toml",
        "ABCDEF",
        "a'%3B%20DROP%20TABLE%20obop_record%3B--",
        &"A".repeat(64),
        &"g".repeat(64),
        &"a".repeat(65),
    ] {
        let (s, body, _) = download(&f, Some(&f.admin), &format!("/api/blobs/{bad}")).await;
        assert_eq!(s, 404, "{bad}");
        assert!(!String::from_utf8_lossy(&body).contains("root:"));
    }
    // A made-up or foreign id cannot be saved into a record.
    let (s, v) = json_call(
        &f.router,
        Some(&f.admin),
        "/api/product/create",
        json!({"name": "X", "image": "a".repeat(64)}),
    )
    .await;
    assert_eq!(s, 400, "{v}");
    let (s, v, _) = upload(
        &f,
        Some(&f.admin),
        "product.image",
        "image/png",
        fx::png(24, 24, 5),
    )
    .await;
    assert_eq!(s, 201);
    let mine = v["id"].as_str().unwrap().to_string();
    // A second manager (the agent holds product.manager) did not upload it and no record holds
    // it yet, so the agent cannot claim it.
    let (s, v) = json_call(
        &f.router,
        Some(&f.agent),
        "/api/product/create",
        json!({"name": "Steal", "image": mine}),
    )
    .await;
    assert_eq!(s, 400, "{v}");
}

#[tokio::test]
async fn uploads_are_rate_limited_per_principal() {
    let f = fixture().await;
    let mut codes = Vec::new();
    for i in 0..45u8 {
        let (s, _, h) = upload(
            &f,
            Some(&f.admin),
            "product.image",
            "image/png",
            fx::png(8, 8, i),
        )
        .await;
        if s == 429 {
            assert!(h.contains_key("retry-after"));
        }
        codes.push(s);
    }
    assert_eq!(codes.iter().filter(|c| **c == 201).count(), 40);
    assert_eq!(codes.iter().filter(|c| **c == 429).count(), 5);
}

#[tokio::test]
async fn meta_and_openapi_describe_image_fields_and_the_blob_endpoints() {
    let f = fixture().await;
    let (_, body, _) = download(&f, Some(&f.admin), "/api/meta").await;
    let meta: J = serde_json::from_slice(&body).unwrap();
    let field = meta["modules"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["models"].as_array().unwrap())
        .find(|m| m["name"] == "product")
        .unwrap()["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "image")
        .unwrap()
        .clone();
    assert_eq!(field["kind"], "image");
    assert_eq!(field["max_bytes"], 5 * 1024 * 1024);
    let (_, body, _) = download(&f, Some(&f.admin), "/api/openapi.json").await;
    let api: J = serde_json::from_slice(&body).unwrap();
    assert!(api["paths"]["/api/blobs"]["post"].is_object());
    assert!(api["paths"]["/api/blobs/{id}"]["get"].is_object());
    assert_eq!(
        api["components"]["schemas"]["product"]["properties"]["image"]["pattern"],
        "^[0-9a-f]{64}$"
    );
}

#[tokio::test]
async fn erasing_a_contact_deletes_its_photo() {
    let f = fixture().await;
    let (s, v, _) = upload(
        &f,
        Some(&f.admin),
        "party.photo",
        "image/png",
        fx::png(16, 16, 9),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    let (s, v) = json_call(
        &f.router,
        Some(&f.admin),
        "/api/party/create",
        json!({"name": "Ada", "email": "ada@example.test", "photo": id}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let party = v["value"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        download(&f, Some(&f.admin), &format!("/api/blobs/{id}"))
            .await
            .0,
        200
    );
    // Erasure is a command that needs approval for agents; an administrator runs it directly.
    let (s, v) = json_call(
        &f.router,
        Some(&f.admin),
        "/api/party/erase",
        json!({"id": party}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        download(&f, Some(&f.admin), &format!("/api/blobs/{id}"))
            .await
            .0,
        404
    );
    let rec = f.app.store.backend().get("party", &party).unwrap().unwrap();
    assert!(rec["photo"].is_null());
    assert_eq!(rec["name"], "[erased]");
}
