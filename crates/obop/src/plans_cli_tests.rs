//! The approvals inbox (D97) at a terminal: the generic call of a catalogue command,
//! `obop call [--token TOKEN] plan <command> [k=v...]`, works for a person and is answered to an
//! agent's token as a command that does not exist. There is no `obop plan ...` sugar yet.

use obop_commands::{CallError, Surface, Value, Vault};
use serde_json::json;

fn opts(db: &str, token: &str) -> crate::Opts {
    crate::Opts {
        db: Some(db.to_string()),
        token: Some(token.to_string()),
        demo: false,
        dev_auth: true,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    }
}

#[test]
fn obop_call_plan_lists_rejects_and_the_agent_reads_what_became_of_its_request() {
    // A directory of this run's own (see `brand_commands_tests` for why it is not removed at once
    // on Windows).
    let scratch = obop_storage::testing::TempPath::dir("obop-plans-cli");
    let db = format!(
        "sqlite://{}",
        scratch.path().join("cli.db").to_str().unwrap()
    );
    let (person, agent) = (opts(&db, "dev-admin"), opts(&db, "dev-agent"));
    // The agent proposes to erase a contact: a plan, kept in the database.
    let plan = {
        let app = crate::open_app(&person).expect("the database opens");
        app.seed_demo();
        let bot = crate::principal(&app, &agent).expect("the dev agent");
        let party = match app.dispatcher.call(
            Surface::Cli,
            &bot,
            &mut Vault::new(),
            "party.list",
            &Value::map([]),
        ) {
            Ok(r) => match r.value {
                Value::List(rows) => match &rows[0] {
                    Value::Map(m) => m["id"].clone(),
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            },
            Err(e) => panic!("{e:?}"),
        };
        match app.dispatcher.call(
            Surface::Cli,
            &bot,
            &mut Vault::new(),
            "party.erase",
            &Value::map([("id", party)]),
        ) {
            Err(CallError::ApprovalRequired { plan_id }) => plan_id,
            other => panic!("expected a plan, got {other:?}"),
        }
    };
    // `obop call --token <person> plan count` and `plan list`.
    let (count, _) = crate::call_json(&person, "plan", "count", &[]).unwrap();
    assert_eq!(count["waiting"], json!(1), "{count}");
    let (list, _) = crate::call_json(&person, "plan", "list", &["limit=5"]).unwrap();
    assert_eq!(list["total"], json!(1), "{list}");
    assert_eq!(list["rows"][0]["id"], json!(plan));
    assert_eq!(list["rows"][0]["command"], json!("party.erase"));
    // To the agent's token the person's commands do not exist: the same refusal, line and exit
    // status as a command that was never defined.
    let unknown = {
        let app = crate::open_app(&agent).unwrap();
        let bot = crate::principal(&app, &agent).unwrap();
        let line = |id: &str| crate::dispatch_call(&app, &bot, id, &Value::map([])).unwrap_err();
        let nothing = line("nothing.list");
        for c in ["list", "count", "approve", "reject"] {
            let (text, status) = line(&format!("plan.{c}"));
            assert_eq!(text, format!("error: UnknownCommand(\"plan.{c}\")"), "{c}");
            assert_eq!(status, nothing.1, "{c}");
        }
        nothing
    };
    assert!(unknown.0.contains("UnknownCommand"));
    assert!(crate::call_json(&agent, "plan", "list", &[]).is_err());
    // `obop call --token <person> plan reject id=<plan> reason=...`
    let reason = "reason=Keep this contact, it has open invoices";
    let (row, _) =
        crate::call_json(&person, "plan", "reject", &[&format!("id={plan}"), reason]).unwrap();
    assert_eq!(row["status"], json!("rejected"), "{row}");
    // `obop call --token <agent> plan get id=<plan>`: the agent hears it, the reason in its own
    // field.
    let (told, _) = crate::call_json(&agent, "plan", "get", &[&format!("id={plan}")]).unwrap();
    assert_eq!(told["status"], json!("rejected"), "{told}");
    assert_eq!(
        told["reason"],
        json!("Keep this contact, it has open invoices")
    );
    assert!(
        told["instruction"]
            .as_str()
            .unwrap()
            .starts_with("A person rejected this")
    );
    // Decided: a second reject says by whom, to the person.
    assert!(crate::call_json(&person, "plan", "reject", &[&format!("id={plan}")]).is_err());
}
