//! SSH CONNECT tunnel for git's transport through sandme's proxy.

mod client;

pub(crate) use client::{PROXY_COMMAND_ARG, git_ssh_command, run};
