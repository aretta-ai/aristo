//! Subcommand handlers. One module per command. Each module's `run` fn
//! is the entry point invoked from `lib::dispatch`.

pub(crate) mod index;
pub(crate) mod init;
pub(crate) mod install_skills;
pub(crate) mod lang;
pub(crate) mod list;
pub(crate) mod show;
pub(crate) mod stamp;
pub(crate) mod status;
