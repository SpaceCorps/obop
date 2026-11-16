//! The first-administrator set-up (D96, PROPOSED): one refusal test per rule of the design, and the
//! way through. The rules are listed in `setup.rs`; each test names the rule it holds.

use super::*;
use crate::mfa::EVENT_TABLE;
use crate::mfa_tests::{T0, enforcing};
use crate::setup::{SetupOpened, code_key_for_test};
use obop_commands::brand;
use obop_commands::{ADMIN_GROUP, MfaApi, SetupApi, SetupError, SetupRequest};
use obop_storage::testing::{Flaky, Mode};

const PW: &str = "a long enough password 1";

/// The groups `obop admin create NAME --all-groups` would give on a small instance.
fn all_groups() -> Vec<String> {
    [ADMIN_GROUP, "base.user", "sales.user"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn with_config(b: Arc<dyn obop_storage::Backend>, config: AuthConfig) -> Auth {
    let a = Auth::with_test_config(b, config);
    a.set_clock_for_test(T0);
    a.enable_setup(all_groups()).unwrap();
    a
}

/// An empty instance with the public development key and the administrator policy on.
fn empty() -> (Auth, Arc<dyn obop_storage::Backend>) {
    let b = obop_storage::open("memory:").unwrap();
    (with_config(b.clone(), enforcing()), b)
}

fn no_key() -> AuthConfig {
    AuthConfig {
        keys: obop_keys::KeyRing::default(),
        ..enforcing()
    }
}

fn open(a: &Auth) -> String {
    match a.open_setup().unwrap() {
        SetupOpened::Code(c) => c,
        other => panic!("expected a code, got {other:?}"),
    }
}

fn req<'a>(source: &'a str, code: &'a str, user: &'a str) -> SetupRequest<'a> {
    SetupRequest {
        source,
        code,
        username: user,
        password: PW,
        instance_name: None,
    }
}

fn accounts(b: &Arc<dyn obop_storage::Backend>) -> usize {
    b.count("auth_user").unwrap()
}

fn events(b: &Arc<dyn obop_storage::Backend>, kind: &str) -> Vec<String> {
    b.list(EVENT_TABLE)
        .unwrap()
        .into_iter()
        .filter(|(_, e)| e["kind"].as_str() == Some(kind))
        .map(|(_, e)| e["detail"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn brand_row(b: &Arc<dyn obop_storage::Backend>) -> bool {
    b.get(brand::MODEL, brand::ROW_ID).unwrap().is_some()
}

// ---- rule 1: the proof ----------------------------------------------------------------------

#[test]
fn a_code_is_twenty_symbols_of_a_thirty_two_symbol_alphabet_in_groups_of_five() {
    let (a, _) = empty();
    let c = open(&a);
    let groups: Vec<&str> = c.split('-').collect();
    assert_eq!(groups.len(), 4);
    assert!(groups.iter().all(|g| g.len() == 5));
    assert!(
        c.chars()
            .filter(|c| *c != '-')
            .all(|c| "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(c))
    );
    assert_ne!(open(&a), c, "every start makes a new code");
}

#[test]
fn only_the_hash_of_a_code_is_kept_and_only_in_the_expiring_store() {
    let (a, b) = empty();
    let c = open(&a);
    let held = b.kv_scan("").unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].0, code_key_for_test(&c));
    assert!(!held[0].0.contains(&c) && !held[0].1.contains(&c));
    assert_eq!(b.total().unwrap(), events(&b, "setup_open").len());
}

#[test]
fn no_code_is_made_while_an_account_exists_or_without_a_server_key() {
    let (a, b) = empty();
    a.create_user("ada", PW, &[]).unwrap();
    assert_eq!(a.open_setup().unwrap(), SetupOpened::Closed);
    assert!(b.kv_scan("auth.setup.code:").unwrap().is_empty());

    let b = obop_storage::open("memory:").unwrap();
    let a = with_config(b.clone(), no_key());
    assert_eq!(a.open_setup().unwrap(), SetupOpened::NeedsSecretKey);
    assert!(b.kv_scan("auth.setup.code:").unwrap().is_empty());
}

#[test]
fn spaces_hyphens_and_small_letters_in_a_typed_code_do_not_matter() {
    let (a, b) = empty();
    let c = open(&a);
    let typed = format!(" {} ", c.replace('-', " ").to_lowercase());
    a.complete(&req("192.0.2.1", &typed, "root")).unwrap();
    assert_eq!(accounts(&b), 1);
}

// ---- rule 4: one answer for every code that is not in force ----------------------------------

#[test]
fn a_wrong_an_expired_a_missing_and_a_burnt_code_and_no_code_at_all_get_the_same_answer() {
    let (a, b) = empty();
    // No code in force.
    assert_eq!(
        a.complete(&req("192.0.2.1", "AAAAA-AAAAA-AAAAA-AAAAA", "root")),
        Err(SetupError::CodeInvalid)
    );
    // Expired: a code that lived 30 ms.
    let expired = match a.open_setup_for(30).unwrap() {
        SetupOpened::Code(c) => c,
        other => panic!("{other:?}"),
    };
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert_eq!(
        a.complete(&req("192.0.2.1", &expired, "root")),
        Err(SetupError::CodeInvalid)
    );
    let good = open(&a);
    for wrong in ["", "   ", "AAAAA-AAAAA-AAAAA-AAAAA", &good[..10]] {
        assert_eq!(
            a.complete(&req("192.0.2.1", wrong, "root")),
            Err(SetupError::CodeInvalid),
            "{wrong:?}"
        );
    }
    assert_eq!(accounts(&b), 0);
    a.complete(&req("192.0.2.1", &good, "root")).unwrap();
    // Burnt: the account exists now, so the door is closed before the code is looked at; and the
    // code itself is gone.
    assert!(b.kv_scan("auth.setup.code:").unwrap().is_empty());
}

// ---- rule 3: nothing is looked at, hashed or written for a caller without the code -----------

#[test]
fn bad_input_with_a_wrong_code_is_told_the_code_is_wrong_and_nothing_is_written() {
    let (a, b) = empty();
    let _ = open(&a);
    let mut r = req("192.0.2.1", "AAAAA-AAAAA-AAAAA-AAAAA", "not a name");
    r.password = "short";
    r.instance_name = Some("<b>bad</b>");
    assert_eq!(a.complete(&r), Err(SetupError::CodeInvalid));
    assert_eq!(accounts(&b), 0);
    assert!(!brand_row(&b));
}

#[test]
fn with_the_code_bad_input_is_refused_before_anything_is_written() {
    let (a, b) = empty();
    let c = open(&a);
    let mut r = req("192.0.2.1", &c, "not a name");
    assert!(matches!(a.complete(&r), Err(SetupError::Invalid(m)) if m.contains("username")));
    r.username = "root";
    r.password = "short";
    assert!(matches!(a.complete(&r), Err(SetupError::Invalid(m)) if m.contains("12")));
    r.password = PW;
    r.instance_name = Some("<b>bad</b>");
    assert!(matches!(a.complete(&r), Err(SetupError::Invalid(m)) if m.starts_with("name")));
    assert_eq!(accounts(&b), 0);
    assert!(
        !brand_row(&b),
        "a refused name writes no brand and no account"
    );
    // The code still works after refused input.
    r.instance_name = None;
    a.complete(&r).unwrap();
}

// ---- rule 2: closed for good, and a strict count -------------------------------------------

#[test]
fn any_account_closes_both_routes_however_it_was_made() {
    // The command line, the control plane (a stored hash) and the set-up page itself.
    for how in ["cli", "hash", "page"] {
        let (a, b) = empty();
        let c = open(&a);
        assert!(SetupApi::status(&a).unwrap().open);
        match how {
            "cli" => a
                .create_user_from_cli("op", PW, &["base.user".into()])
                .unwrap(),
            "hash" => {
                let h = a.hash_for_test(PW);
                a.create_user_hashed("owner", &h, &[ADMIN_GROUP.into()])
                    .unwrap()
            }
            _ => {
                a.complete(&req("192.0.2.1", &c, "root")).unwrap();
            }
        }
        let s = SetupApi::status(&a).unwrap();
        assert!(!s.open && !s.code_active && !s.needs_secret_key, "{how}");
        // The right code no longer helps, if it is still in force at all.
        assert_eq!(
            a.complete(&req("192.0.2.1", &c, "second")),
            Err(SetupError::Closed),
            "{how}"
        );
        assert_eq!(accounts(&b), 1, "{how}");
    }
}

#[test]
fn a_count_of_accounts_the_store_refuses_is_an_error_and_never_open() {
    let (flaky, b) = Flaky::memory();
    let a = with_config(b.clone(), enforcing());
    let c = open(&a);
    flaky.refuse_model("auth_user", Mode::Reads);
    // `user_count` would say 0 here: the door must not use it.
    assert_eq!(a.user_count(), 0);
    assert!(matches!(
        SetupApi::status(&a),
        Err(SetupError::Unavailable(_))
    ));
    assert!(matches!(
        a.complete(&req("192.0.2.1", &c, "root")),
        Err(SetupError::Unavailable(_))
    ));
    assert!(a.open_setup().is_err(), "no code is made on a guess either");
    flaky.allow_models();
    assert_eq!(accounts(&b), 0);
}

// ---- rule 4: guess limits that fail closed --------------------------------------------------

#[test]
fn the_eleventh_attempt_from_one_address_is_refused_with_the_wait_even_with_the_right_code() {
    let (a, b) = empty();
    let c = open(&a);
    for _ in 0..ADDR_MAX {
        assert_eq!(
            a.complete(&req("192.0.2.1", "AAAAA", "root")),
            Err(SetupError::CodeInvalid)
        );
    }
    match a.complete(&req("192.0.2.1", &c, "root")) {
        Err(SetupError::RateLimited(s)) => assert!((1..=600).contains(&s)),
        other => panic!("{other:?}"),
    }
    assert_eq!(accounts(&b), 0);
    // Another address is not held back by it.
    a.complete(&req("192.0.2.2", &c, "root")).unwrap();
}

#[test]
fn the_sixty_first_attempt_in_an_hour_is_refused_for_every_address() {
    let (a, b) = empty();
    let c = open(&a);
    for i in 0..ALL_MAX {
        let addr = format!("198.51.100.{i}");
        assert_eq!(
            a.complete(&req(&addr, "AAAAA", "root")),
            Err(SetupError::CodeInvalid)
        );
    }
    assert!(matches!(
        a.complete(&req("203.0.113.9", &c, "root")),
        Err(SetupError::RateLimited(_))
    ));
    assert_eq!(accounts(&b), 0);
}

#[test]
fn a_counter_the_store_will_not_keep_refuses_the_attempt() {
    // The calls of an attempt: the count of accounts, then the counter of the address. The second
    // is refused: `limit::hit` would let it through, this door does not.
    let (flaky, b) = Flaky::memory();
    let a = with_config(b.clone(), enforcing());
    let c = open(&a);
    flaky.refuse_call_after(1);
    assert!(matches!(
        a.complete(&req("192.0.2.1", &c, "root")),
        Err(SetupError::Unavailable(_))
    ));
    // And the counter of the whole instance (third call).
    flaky.refuse_call_after(2);
    assert!(matches!(
        a.complete(&req("192.0.2.1", &c, "root")),
        Err(SetupError::Unavailable(_))
    ));
    assert_eq!(accounts(&b), 0);
    a.complete(&req("192.0.2.1", &c, "root")).unwrap();
}

// ---- rule 5: a server key is required -------------------------------------------------------

#[test]
fn without_a_server_key_set_up_is_refused_and_says_so() {
    let b = obop_storage::open("memory:").unwrap();
    let a = with_config(b.clone(), no_key());
    let s = SetupApi::status(&a).unwrap();
    assert!(s.open && s.needs_secret_key && !s.code_active);
    // Even a code that a process with a key made does not help a process without one.
    let keyed = with_config(b.clone(), enforcing());
    let c = open(&keyed);
    assert_eq!(
        a.complete(&req("192.0.2.1", &c, "root")),
        Err(SetupError::NeedsSecretKey)
    );
    assert_eq!(accounts(&b), 0);
}

#[test]
fn without_a_server_key_the_policy_would_give_a_full_session_which_is_why_the_key_is_required() {
    // What the refusal prevents: the two-factor policy is not applied without a key.
    let b = obop_storage::open("memory:").unwrap();
    let a = Auth::with_test_config(b, no_key());
    a.create_user("root", PW, &[ADMIN_GROUP.into()]).unwrap();
    let LoginOutcome::Session {
        mfa_enrolment_required,
        ..
    } = a.login_step("root", PW).unwrap()
    else {
        panic!()
    };
    assert!(!mfa_enrolment_required);
}

// ---- rule 6: one winner ---------------------------------------------------------------------

#[test]
fn two_processes_racing_with_the_right_code_make_one_account() {
    for _ in 0..10 {
        let b = obop_storage::open("memory:").unwrap();
        let one = Arc::new(with_config(b.clone(), enforcing()));
        let two = Arc::new(with_config(b.clone(), enforcing()));
        let c = open(&one);
        let run = |a: Arc<Auth>, user: &'static str, c: String| {
            std::thread::spawn(move || a.complete(&req("192.0.2.1", &c, user)).map(|_| ()))
        };
        let x = run(one, "ada", c.clone());
        let y = run(two, "bob", c);
        let results = [x.join().unwrap(), y.join().unwrap()];
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "{results:?}"
        );
        assert!(
            results
                .iter()
                .any(|r| matches!(r, Err(SetupError::Closed | SetupError::CodeInvalid)))
        );
        assert_eq!(accounts(&b), 1);
        assert_eq!(events(&b, "setup_complete").len(), 1);
    }
}

#[test]
fn a_second_process_accepts_a_code_the_first_printed() {
    let b = obop_storage::open("memory:").unwrap();
    let first = with_config(b.clone(), enforcing());
    let second = with_config(b.clone(), enforcing());
    let c = open(&first);
    let other = open(&second);
    assert!(SetupApi::status(&second).unwrap().code_active);
    second.complete(&req("192.0.2.1", &c, "root")).unwrap();
    // Every code is burnt by the creation, the other process's too.
    assert!(b.kv_scan("auth.setup.code:").unwrap().is_empty());
    assert_eq!(
        first.complete(&req("192.0.2.1", &other, "late")),
        Err(SetupError::Closed)
    );
}

#[test]
fn an_account_made_while_the_page_waits_for_the_lock_closes_it() {
    // The command line holds the lock and makes an account; the page, which passed its first
    // checks, counts again inside the lock and makes nothing.
    let (a, b) = empty();
    let a = Arc::new(a);
    let c = open(&a);
    let held = b.lock(SETUP_LOCK).unwrap();
    let page = {
        let a = a.clone();
        std::thread::spawn(move || a.complete(&req("192.0.2.1", &c, "root")).map(|_| ()))
    };
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(accounts(&b), 0, "the page waits for the lock");
    a.create_user("op", PW, &[ADMIN_GROUP.into()]).unwrap();
    drop(held);
    assert_eq!(page.join().unwrap(), Err(SetupError::Closed));
    assert_eq!(accounts(&b), 1);
}

#[test]
fn the_command_line_takes_the_same_lock_and_writes_its_events() {
    let (a, b) = empty();
    let a = Arc::new(a);
    let _ = open(&a);
    let held = b.lock(SETUP_LOCK).unwrap();
    let cli = {
        let a = a.clone();
        std::thread::spawn(move || a.create_user_from_cli("op", PW, &[ADMIN_GROUP.into()]))
    };
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(accounts(&b), 0, "the command line waits for the lock");
    drop(held);
    cli.join().unwrap().unwrap();
    assert_eq!(events(&b, "user_create").len(), 1);
    assert_eq!(events(&b, "setup_complete").len(), 1, "the first account");
    assert!(
        b.kv_scan("auth.setup.code:").unwrap().is_empty(),
        "codes burnt"
    );
    // A second account from the command line is an ordinary one.
    a.create_user_from_cli("sam", PW, &[]).unwrap();
    assert_eq!(events(&b, "user_create").len(), 2);
    assert_eq!(events(&b, "setup_complete").len(), 1);
    // Same messages as before for a refused name.
    assert_eq!(
        a.create_user_from_cli("a b", PW, &[]).unwrap_err(),
        "username must be letters, digits or . _ - @"
    );
}

// ---- rules 7 and 8: the account, the name, the session --------------------------------------

#[test]
fn the_first_administrator_gets_the_groups_of_all_groups_and_a_session_limited_to_enrolment() {
    let (a, b) = empty();
    let c = open(&a);
    let mut r = req("192.0.2.1", &c, " Root ");
    r.instance_name = Some("Acme Test");
    let LoginOutcome::Session {
        token,
        mfa_enrolment_required,
        password_change_required,
    } = a.complete(&r).unwrap()
    else {
        panic!("a session")
    };
    assert!(mfa_enrolment_required && !password_change_required);
    let p = a.resolve(&token).unwrap();
    assert_eq!(p.id, "root");
    assert!(p.mfa_limited, "the session can only enrol");
    let u = b.get("auth_user", "root").unwrap().unwrap();
    assert_eq!(u["groups"], json!(all_groups()));
    assert_eq!(u["created_by"], json!("setup"));
    assert_eq!(u["must_change_password"], json!(false));
    // The brand has the name before the first enrolment, and the enrolment uses it.
    assert_eq!(brand::strict(&*b).unwrap().name(), "Acme Test");
    let begun = a.begin(&p).unwrap();
    assert_eq!(begun["issuer"], json!("Acme Test"));
    assert!(
        begun["uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/Acme%20Test:root?")
    );
}

#[test]
fn with_the_policy_off_the_answer_is_a_full_session() {
    let b = obop_storage::open("memory:").unwrap();
    let a = with_config(
        b.clone(),
        AuthConfig {
            default_require_admins: false,
            ..enforcing()
        },
    );
    let c = open(&a);
    let LoginOutcome::Session {
        token,
        mfa_enrolment_required,
        ..
    } = a.complete(&req("192.0.2.1", &c, "root")).unwrap()
    else {
        panic!()
    };
    assert!(!mfa_enrolment_required);
    assert!(!a.resolve(&token).unwrap().mfa_limited);
}

#[test]
fn set_up_is_off_until_the_composition_root_gives_groups_with_the_administrator_group() {
    let b = obop_storage::open("memory:").unwrap();
    let a = Auth::with_test_config(b, enforcing());
    assert!(a.setup().is_none());
    assert!(a.enable_setup(vec!["base.user".into()]).is_err());
    assert!(
        a.enable_setup(vec!["*".into()]).is_err(),
        "`*` is not the administrator group"
    );
    assert!(a.setup().is_none());
    a.enable_setup(all_groups()).unwrap();
    assert!(a.setup().is_some());
}

// ---- rule 10 and rule 1: the log, and the code never in it -----------------------------------

#[test]
fn the_log_holds_open_refused_and_complete_and_never_the_code() {
    let (a, b) = empty();
    let c = open(&a);
    let _ = open(&a); // a second start in ten minutes writes no second line
    assert_eq!(events(&b, "setup_open").len(), 1);
    let mut answers = Vec::new();
    for _ in 0..3 {
        answers.push(format!(
            "{:?}",
            a.complete(&req("192.0.2.1", "WRONG", "root"))
        ));
    }
    assert_eq!(
        events(&b, "setup_refused").len(),
        1,
        "one line per address and reason in ten minutes"
    );
    let _ = a.complete(&req("192.0.2.7", "WRONG", "root"));
    assert_eq!(events(&b, "setup_refused").len(), 2, "another address");
    answers.push(format!("{:?}", a.complete(&req("192.0.2.1", &c, "root"))));
    assert_eq!(events(&b, "setup_complete").len(), 1);
    assert_eq!(events(&b, "user_create").len(), 1);
    answers.push(format!("{:?}", a.complete(&req("192.0.2.1", &c, "root"))));
    let normalised: String = c.chars().filter(|x| *x != '-').collect();
    let hash = code_key_for_test(&c);
    let hash = hash.trim_start_matches("auth.setup.code:");
    for (_, e) in b.list(EVENT_TABLE).unwrap() {
        let row = serde_json::to_string(&e).unwrap();
        for secret in [c.as_str(), normalised.as_str(), hash, &c[..5]] {
            assert!(!row.contains(secret), "{row}");
        }
        assert!(!row.contains(PW));
        assert!(!row.contains("192.0.2."), "no address in the text: {row}");
    }
    for answer in &answers {
        assert!(!answer.contains(&normalised) && !answer.contains(&c[..5]));
    }
}

#[test]
fn refused_lines_stop_at_thirty_an_hour_and_say_so() {
    let (a, b) = empty();
    let _ = open(&a);
    for i in 0..40 {
        let _ = a.complete(&req(&format!("198.51.100.{i}"), "WRONG", "root"));
    }
    let lines = events(&b, "setup_refused");
    assert_eq!(lines.len(), 31);
    assert!(lines.iter().any(|l| l.contains("ceiling of 30 an hour")));
}

#[test]
fn the_any_group_of_all_groups_is_not_written_into_the_account() {
    let b = obop_storage::open("memory:").unwrap();
    let a = Auth::with_test_config(b.clone(), enforcing());
    let mut g = all_groups();
    g.push("*".into());
    a.enable_setup(g).unwrap();
    let c = open(&a);
    a.complete(&req("192.0.2.1", &c, "root")).unwrap();
    let u = b.get("auth_user", "root").unwrap().unwrap();
    assert_eq!(u["groups"], json!(all_groups()));
}
