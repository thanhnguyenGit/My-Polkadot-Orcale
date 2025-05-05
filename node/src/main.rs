//! Substrate Parachain Node Template CLI

#![warn(missing_docs)]

use std::process::{Command, Stdio};

mod chain_spec;
mod cli;
mod command;
mod rpc;
mod service;

fn main() -> sc_cli::Result<()> {
	command::run()
}
