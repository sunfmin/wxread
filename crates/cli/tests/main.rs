//! The tests of the CLI, one program: each runs the real binary and asserts
//! its JSON and exit code.
#![cfg(unix)]

mod chats;
mod common;
mod contacts;
mod doctor;
mod errors;
mod help;
mod messages;
mod release;
mod synthetic;
mod version;
mod voice;
