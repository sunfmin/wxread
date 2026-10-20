//! The tests of the CLI, one program: each runs the real binary and asserts
//! its JSON and exit code.
#![cfg(unix)]

mod common;
mod doctor;
mod errors;
mod help;
mod version;
