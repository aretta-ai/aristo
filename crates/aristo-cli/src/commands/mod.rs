//! Subcommand handlers. One module per command. Each module's `run` fn
//! is the entry point invoked from `lib::dispatch`.

pub(crate) mod badge;
pub(crate) mod critique;
pub(crate) mod doc;
pub(crate) mod graph;
pub(crate) mod index;
pub(crate) mod init;
pub(crate) mod install_skills;
pub(crate) mod install_skills_hook;
pub(crate) mod lang;
pub(crate) mod lint;
pub(crate) mod list;
pub(crate) mod rename;
pub(crate) mod session;
pub(crate) mod show;
pub(crate) mod stamp;
pub(crate) mod status;
pub(crate) mod verify;
