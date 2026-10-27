//! `set_production` is a flag of the whole process, so it is tested here, in a process of its own
//! (an integration test is its own binary): no unit test of the library may flip it while others
//! open databases beside it.

use obop_storage::{open, open_with, production, set_production};

/// A database made for this test on the server `OBOP_TEST_POSTGRES_URL` names, dropped at the end
/// (`TestDb` is for the tests inside the library; this one needs only the public API).
struct Scratch {
    admin: String,
    name: String,
    url: String,
}

impl Scratch {
    fn new(admin: &str) -> Scratch {
        let name = format!(
            "obop_prod_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() % 1_000_000_000)
        );
        obop_storage::tls::connect(admin)
            .unwrap()
            .batch_execute(&format!("CREATE DATABASE {name}"))
            .unwrap();
        let (head, query) = match admin.split_once('?') {
            Some((h, q)) => (h, format!("?{q}")),
            None => (admin, String::new()),
        };
        let (server, _) = head.rsplit_once('/').unwrap();
        Scratch {
            url: format!("{server}/{name}{query}"),
            admin: admin.to_string(),
            name,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(mut c) = obop_storage::tls::connect(&self.admin) {
            let _ = c.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
        }
    }
}

#[test]
fn a_production_process_does_not_create_the_schema_but_a_setup_command_still_can() {
    let Ok(admin) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let db = Scratch::new(&admin);
    assert!(!production(), "this process starts as a development one");

    // Not production: `open` creates the schema, as it always did.
    let fresh = Scratch::new(&admin);
    open(&fresh.url)
        .unwrap()
        .insert("m", "a", &Default::default())
        .unwrap();

    set_production(true);
    assert!(production());
    // Production: an empty database is an error that says what is missing, and nothing is made.
    let e = open(&db.url)
        .err()
        .expect("production does not create the schema");
    assert!(e.contains("missing: base"), "{e}");
    let e = open(&db.url).err().expect("and still does not");
    assert!(e.contains("missing: base"), "{e}");
    // `open_with` is for the commands that set a database up: it creates whatever is set.
    open_with(&db.url, None).unwrap();
    // Now the schema is there, and a production process opens and uses it.
    let b = open(&db.url).unwrap();
    b.insert("m", "a", &Default::default()).unwrap();
    assert!(b.get("m", "a").unwrap().is_some());
    // The byte store follows the same rule: its table is not made by a production process, and
    // the database was set up without it (`open_with` makes the record store's tables only).
    let e = obop_storage::blob::open_bytes(&db.url, None)
        .err()
        .expect("production does not create the byte store's table");
    assert!(e.contains("missing: blob"), "{e}");
    // `schema::owner_install` is what makes every table, and then it opens.
    obop_storage::schema::owner_install(&db.url).unwrap();
    obop_storage::blob::open_bytes(&db.url, None).unwrap();
    // The other stores have no schema to create.
    open("memory:").unwrap();
    open("sqlite::memory:").unwrap();

    set_production(false);
    assert!(!production());
    open(&db.url).unwrap();
}
