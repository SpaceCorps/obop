//! Step 2: values sealed before the key ring (`v1.` second-factor secrets, `obs1:` connector
//! secrets) are re-sealed in the key ring's envelope with the key that opens them.

use crate::Ctx;
use obop_keys::reseal;

pub(crate) fn run(c: &Ctx) -> Result<String, String> {
    if !reseal::has_legacy(&**c.backend)? {
        return Ok("no secret in an older format".into());
    }
    if c.ring.is_empty() {
        return Err(
            "the stored secrets need the server key to be upgraded: set OBOP_SECRET_KEY \
             (and OBOP_SECRET_KEY_PREVIOUS for earlier keys)"
                .into(),
        );
    }
    let r = reseal::run(
        c.backend,
        c.ring,
        reseal::Options {
            only_legacy: true,
            dry_run: c.dry_run,
            ..reseal::Options::default()
        },
        &mut |_| {},
    )?;
    if !r.unopenable.is_empty() {
        return Err(format!(
            "{} secret(s) cannot be opened with the configured keys (first: {}); put the key \
             that sealed them in OBOP_SECRET_KEY_PREVIOUS",
            r.unopenable.len(),
            r.unopenable[0].place
        ));
    }
    if r.changed_meanwhile > 0 {
        return Err(format!(
            "{} secret(s) changed while they were re-sealed; run the migration again",
            r.changed_meanwhile
        ));
    }
    Ok(format!("{} secret(s) re-sealed", r.resealed))
}
