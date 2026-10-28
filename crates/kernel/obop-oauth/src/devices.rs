//! Ending a device through the `device.revoke` command (the D64 amendment). The commands of the
//! `device` model are registered by `obop-pkg`; this is the part that is the OAuth server's own: the
//! refresh-token family, the access token in circulation and the record, ended by the one code that
//! ends a connected application (`OAuth::revoke_device`).

use crate::OAuth;
use obop_commands::devices::DevicesApi;
use obop_storage::fault::Fault;

impl DevicesApi for OAuth {
    fn revoke_device(&self, id: &str, actor: &str) -> Result<(), String> {
        OAuth::revoke_device(self, id, actor).map_err(|e| {
            if e.status == 503 {
                // The code word of the storage faults, so that the surfaces answer with a 503 and
                // the time to wait, not with a refusal of the command.
                Fault::Busy.message(&e.description)
            } else {
                e.description
            }
        })
    }
}
