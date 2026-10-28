//! The Mileage module (docs/spec/mobile.md 5, 8.6) over the real HTTP server with real tokens:
//! the walk of a trip as the Android app and the back office make it, the code of the retryable
//! refusal, and who may reach whose trip on REST, MCP and the command line. The rules themselves
//! are tested on the dispatcher in `crates/modules/mileage/src/tests.rs`.

use crate::app::App;
use crate::mfa_tests::{PW, Rig, post, rig, token};
use obop_commands::{Principal, Surface};
use serde_json::{Value as J, json};

const DRIVER: &str = "mileage.driver";
const REVIEWER: &str = "mileage.reviewer";
const ADMIN: &str = "mileage.admin";

fn user(rig: &Rig, name: &str, groups: &[&str]) {
    let groups: Vec<String> = groups.iter().map(|g| (*g).to_string()).collect();
    rig.auth.create_user(name, PW, &groups).unwrap();
}

/// Ann and Bob drive, Rita reviews, Ada administers the module.
fn people(rig: &Rig) {
    user(rig, "driver_ann", &[DRIVER]);
    user(rig, "driver_bob", &[DRIVER]);
    user(rig, "rita", &[DRIVER, REVIEWER]);
    user(rig, "ada_mileage", &[DRIVER, ADMIN]);
}

fn stamp(seq: i64) -> String {
    // 2026-10-15T09:00:00Z plus five seconds a point.
    format!("2026-10-15T09:{:02}:{:02}Z", seq * 5 / 60, seq * 5 % 60)
}

fn point(trip: &str, seq: i64) -> J {
    json!({
        "trip": trip,
        "seq": seq,
        "recorded_at": stamp(seq),
        "lat": format!("{:.7}", 52.0 + 0.0002 * seq as f64),
        "lon": "13.0000000",
        "accuracy_m": "5",
    })
}

async fn make_trip(r: &axum::Router, t: &str, client: &str) -> String {
    let (s, b) = post(
        r,
        Some(t),
        "/api/mileage_trip/bulk_create",
        json!({
            "idempotency_key": format!("trip:{client}"),
            "rows": [{"client_trip_id": client, "started_at": "2026-10-15T09:00:00Z", "purpose": "business"}],
        }),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    b["value"]["ids"][0].as_str().unwrap().to_string()
}

async fn send_points(r: &axum::Router, t: &str, trip: &str, from: i64, to: i64) -> (u16, J) {
    let rows: Vec<J> = (from..=to).map(|s| point(trip, s)).collect();
    post(
        r,
        Some(t),
        "/api/mileage_point/bulk_create",
        json!({"idempotency_key": format!("points:{trip}:{from}-{to}"), "rows": rows}),
    )
    .await
}

async fn act(r: &axum::Router, t: &str, action: &str, body: J) -> (u16, J) {
    post(r, Some(t), &format!("/api/mileage_trip/{action}"), body).await
}

async fn state_of(r: &axum::Router, t: &str, id: &str) -> String {
    let (s, b) = post(r, Some(t), "/api/mileage_trip/get", json!({"id": id})).await;
    assert_eq!(s, 200, "{b}");
    b["value"]["state"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn rest_a_trip_walks_from_the_phone_to_a_confirmed_total() {
    let rig = rig(false);
    people(&rig);
    let r = &rig.r;
    let (ann, rita) = (token(r, "driver_ann").await, token(r, "rita").await);

    // Start and a first batch of six of ten points, as the outbox would send them.
    let trip = make_trip(r, &ann, "0192f000-0000-7000-8000-00000000a001").await;
    let (s, b) = send_points(r, &ann, &trip, 1, 6).await;
    assert_eq!((s, b["value"]["created"].clone()), (200, json!(6)), "{b}");

    // End while four are missing: a 409 with a stable code and nothing changed.
    let end = json!({"id": trip, "expected_points": 10, "last_seq": 10});
    let (s, b) = act(r, &ann, "end", end.clone()).await;
    assert_eq!(s, 409, "{b}");
    assert_eq!(b["code"], "incomplete", "{b}");
    assert!(b["hint"].as_str().unwrap().ends_with('.'), "{b}");
    assert_eq!(state_of(r, &ann, &trip).await, "recording");

    // The rest arrives, a replay of the first batch is the first answer, and the end goes through.
    let (s, b) = send_points(r, &ann, &trip, 7, 10).await;
    assert_eq!((s, b["value"]["created"].clone()), (200, json!(4)), "{b}");
    let (s, b) = send_points(r, &ann, &trip, 1, 6).await;
    assert_eq!(
        (s, b["value"]["replayed"].clone()),
        (200, json!(true)),
        "{b}"
    );
    let (s, b) = act(r, &ann, "end", end).await;
    assert_eq!(
        (s, b["value"]["state"].clone()),
        (200, json!("ended")),
        "{b}"
    );
    let (_, got) = post(r, Some(&ann), "/api/mileage_trip/get", json!({"id": trip})).await;
    assert_eq!(got["value"]["distance_m"], "200.15", "{got}");
    assert_eq!(got["value"]["point_count"], 10);

    // A refusal for good is a plain 400, not the retryable code.
    let (s, b) = act(r, &ann, "end", json!({"id": trip})).await;
    assert_eq!(s, 400, "{b}");
    assert!(b.get("code").is_none_or(J::is_null), "{b}");

    // Review: the driver cannot decide; the reviewer can; the total moves.
    assert_eq!(act(r, &ann, "submit", json!({"id": trip})).await.0, 200);
    let totals = || async {
        let (s, b) = post(
            r,
            Some(&rita),
            "/api/mileage_trip/search",
            json!({
                "filter": [{"field": "state", "op": "eq", "value": "confirmed"}],
                "aggregate": {
                    "group_by": [{"field": "confirmed_at", "bucket": "month"}],
                    "measures": [{"op": "sum", "field": "distance_m"}],
                },
            }),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        b["value"]["groups"].clone()
    };
    assert_eq!(totals().await, json!([]));
    assert_ne!(act(r, &ann, "confirm", json!({"id": trip})).await.0, 200);
    let (s, b) = act(r, &rita, "confirm", json!({"id": trip})).await;
    assert_eq!(
        (s, b["value"]["state"].clone()),
        (200, json!("confirmed")),
        "{b}"
    );
    let groups = totals().await;
    assert_eq!(
        groups[0]["measures"]["sum:distance_m"], "200.15",
        "{groups}"
    );
    assert_eq!(groups[0]["count"], 1);
}

#[tokio::test]
async fn rest_another_driver_cannot_reach_a_trip_by_any_command() {
    let rig = rig(false);
    people(&rig);
    let r = &rig.r;
    let (ann, bob, rita, ada) = (
        token(r, "driver_ann").await,
        token(r, "driver_bob").await,
        token(r, "rita").await,
        token(r, "ada_mileage").await,
    );
    let trip = make_trip(r, &ann, "0192f000-0000-7000-8000-00000000b001").await;
    assert_eq!(send_points(r, &ann, &trip, 1, 10).await.0, 200);
    let (_, p) = post(r, Some(&ann), "/api/mileage_point/list", json!({})).await;
    let point_id = p["value"][0]["id"].as_str().unwrap().to_string();

    // What Bob gets for Ann's recording trip is what he gets for a trip that is not there.
    for (path, extra) in [
        ("mileage_trip/get", json!({})),
        ("mileage_trip/update", json!({"purpose": "private"})),
        ("mileage_trip/delete", json!({})),
        (
            "mileage_trip/end",
            json!({"expected_points": 10, "last_seq": 10}),
        ),
        ("mileage_trip/submit", json!({})),
        ("mileage_trip/discard", json!({})),
        ("mileage_trip/discard_stuck", json!({})),
        ("mileage_trip/confirm", json!({})),
        ("mileage_trip/reject", json!({})),
        ("mileage_trip/reopen", json!({})),
        ("mileage_point/get", json!({})),
        ("mileage_point/delete", json!({})),
        ("mileage_point/update", json!({"lat": "1.0000000"})),
    ] {
        let id_for = |real: &str| {
            if path.starts_with("mileage_point") {
                point_id.clone()
            } else {
                real.to_string()
            }
        };
        let mut body = extra.clone();
        body["id"] = J::from(id_for(&trip));
        let (s_real, b_real) = post(r, Some(&bob), &format!("/api/{path}"), body).await;
        let mut gone = extra.clone();
        gone["id"] = J::from("018f0000-0000-7000-8000-000000000000");
        let (s_gone, b_gone) = post(r, Some(&bob), &format!("/api/{path}"), gone).await;
        assert!(s_real >= 400, "{path}: {s_real} {b_real}");
        assert_eq!(
            s_real, s_gone,
            "{path}: a real trip and a missing one answer alike: {b_real} / {b_gone}"
        );
    }
    // Nothing changed, and bob lists and searches nothing of Ann's.
    assert_eq!(state_of(r, &ann, &trip).await, "recording");
    for path in [
        "/api/mileage_trip/list",
        "/api/mileage_trip/search",
        "/api/mileage_point/list",
        "/api/mileage_point/search",
    ] {
        let (s, b) = post(r, Some(&bob), path, json!({})).await;
        assert_eq!(s, 200, "{b}");
        let rows = b["value"]["rows"]
            .as_array()
            .or(b["value"].as_array())
            .unwrap();
        assert!(rows.is_empty(), "{path}: {b}");
    }
    // Nor can Bob send points to Ann's trip.
    let (s, _) = send_points(r, &bob, &trip, 11, 11).await;
    assert!(s >= 400);
    // The reviewer sees no trip that is only recording, and an administrator sees it.
    let (s, _) = post(r, Some(&rita), "/api/mileage_trip/get", json!({"id": trip})).await;
    assert!(s >= 400);
    let (s, b) = post(r, Some(&ada), "/api/mileage_trip/get", json!({"id": trip})).await;
    assert_eq!(s, 200, "{b}");
    // Without a token, or with none of the groups, there is no Mileage at all.
    assert_eq!(
        post(r, None, "/api/mileage_trip/list", json!({})).await.0,
        401
    );
    user(&rig, "nobody", &[]);
    let none = token(r, "nobody").await;
    assert_eq!(
        post(r, Some(&none), "/api/mileage_trip/list", json!({}))
            .await
            .0,
        404
    );
}

#[tokio::test]
async fn rest_an_agent_token_cannot_decide_and_proposes_what_a_person_approves() {
    let rig = rig(false);
    people(&rig);
    let r = &rig.r;
    let (ann, rita) = (token(r, "driver_ann").await, token(r, "rita").await);
    let trip = make_trip(r, &ann, "0192f000-0000-7000-8000-00000000c001").await;
    assert_eq!(send_points(r, &ann, &trip, 1, 10).await.0, 200);
    assert_eq!(
        act(
            r,
            &ann,
            "end",
            json!({"id": trip, "expected_points": 10, "last_seq": 10})
        )
        .await
        .0,
        200
    );
    assert_eq!(act(r, &ann, "submit", json!({"id": trip})).await.0, 200);

    // An agent scoped to the reviewer group, issued for Rita, still only proposes.
    let (s, b) = post(
        r,
        Some(&rita),
        "/api/auth/agent-token",
        json!({"scopes": [DRIVER, REVIEWER]}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let agent = b["token"].as_str().unwrap().to_string();
    let (s, b) = act(r, &agent, "confirm", json!({"id": trip})).await;
    assert_eq!(s, 202, "{b}");
    assert_eq!(state_of(r, &rita, &trip).await, "in_review");
    // An agent of Ann holds only the driver group.
    let (_, b) = post(
        r,
        Some(&ann),
        "/api/auth/agent-token",
        json!({"scopes": [DRIVER]}),
    )
    .await;
    let anns_agent = b["token"].as_str().unwrap().to_string();
    let (s, _) = act(r, &anns_agent, "confirm", json!({"id": trip})).await;
    assert_eq!(s, 404, "the command is not in its scope");
}

#[tokio::test]
async fn mcp_and_cli_offer_each_command_to_the_groups_that_may_run_it() {
    let app = App::new();
    let ann = Principal::user("ann", &[DRIVER]);
    let rita = Principal::user("rita", &[DRIVER, REVIEWER]);
    let ada = Principal::user("ada", &[DRIVER, ADMIN]);
    let nobody = Principal::user("x", &[]);
    for surface in [Surface::Rest, Surface::Mcp, Surface::Cli] {
        let offered = |p: &Principal| -> Vec<String> {
            app.dispatcher
                .catalog(surface, p)
                .into_iter()
                .map(|c| c.id)
                .filter(|c| c.starts_with("mileage_"))
                .collect()
        };
        let has = |l: &[String], c: &str| l.iter().any(|x| x == c);
        let (a, r, d) = (offered(&ann), offered(&rita), offered(&ada));
        for c in [
            "end",
            "submit",
            "discard",
            "bulk_create",
            "update",
            "get",
            "list",
            "search",
        ] {
            assert!(has(&a, &format!("mileage_trip.{c}")), "{surface:?} ann {c}");
        }
        assert!(has(&a, "mileage_point.bulk_create"), "{surface:?}");
        for c in ["confirm", "reject", "reopen", "discard_stuck"] {
            assert!(
                !has(&a, &format!("mileage_trip.{c}")),
                "{surface:?} ann must not be offered {c}"
            );
        }
        assert!(
            has(&r, "mileage_trip.confirm")
                && has(&r, "mileage_trip.reject")
                && has(&r, "mileage_trip.reopen")
        );
        assert!(!has(&r, "mileage_trip.discard_stuck"), "{surface:?}");
        assert!(
            has(&d, "mileage_trip.discard_stuck") && !has(&d, "mileage_trip.confirm"),
            "{surface:?}"
        );
        assert!(offered(&nobody).is_empty(), "{surface:?}");
    }
    // The MCP tools are the same commands under model__command, with the schema of the model.
    let tools = obop_mcp::schema::tools(&app.dispatcher, &ann);
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"mileage_trip__end") && names.contains(&"mileage_point__bulk_create"));
    assert!(!names.contains(&"mileage_trip__confirm"));
    let tool = tools
        .iter()
        .find(|t| t["name"] == "mileage_point__bulk_create")
        .unwrap();
    assert_eq!(tool["inputSchema"]["properties"]["rows"]["maxItems"], 1000);
}

#[tokio::test]
async fn the_module_and_its_fields_are_described_for_the_install_gate_and_the_privacy_review() {
    let rig = rig(false);
    people(&rig);
    let ann = token(&rig.r, "driver_ann").await;
    let (s, meta) = crate::mfa_tests::send(&rig.r, "GET", Some(&ann), "/api/meta", json!({})).await;
    assert_eq!(s, 200);
    let text = meta.to_string();
    assert!(text.contains("mileage_trip") && text.contains("mileage_point"));
    // The positions are the sensitive part: both models say so in their data classes.
    let app = App::new();
    let m = mileage::module();
    let point = m.models.iter().find(|m| m.name == "mileage_point").unwrap();
    for f in ["lat", "lon", "recorded_at", "accuracy_m", "speed_mps"] {
        let field = point.fields.iter().find(|x| x.name == f).unwrap();
        assert_eq!(
            field.class,
            Some(obop_types::DataClass::ConfidentialHr),
            "{f}"
        );
    }
    // A phone is offered the driver group and never the reviewer or administrator group.
    let never = obop_commands::devices::never_for_mobile;
    assert!(never(REVIEWER) && never(ADMIN) && !never(DRIVER));
    assert!(
        app.dispatcher
            .catalog(Surface::Rest, &Principal::user("a", &[DRIVER]))
            .len()
            > 10
    );
}
