//! Step 1: the layout of a database created before versioning is version 1.

use crate::Ctx;

pub(crate) fn run(_: &Ctx) -> Result<String, String> {
    Ok("nothing to change: the stored formats are version 1".into())
}
