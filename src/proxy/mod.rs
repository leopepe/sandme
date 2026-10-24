//! HTTP forward proxy mediating the sandboxed command's network egress.

mod egress;
mod request;
mod server;

pub(crate) use server::{Server, serve};
