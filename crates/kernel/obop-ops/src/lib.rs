//! Operator tools that work on a whole database: backup, restore and verification
//! ([`backup`]), and secret rotation ([`secrets`]). The `obop` binary wires them to the command
//! line; nothing here talks to a network except the database it is given.

pub mod backup;
pub mod secrets;
