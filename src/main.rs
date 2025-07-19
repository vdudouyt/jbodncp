mod filelist;
mod cli;
mod client;
mod server;
mod jbod;

use clap::Parser;
use client::run_client;
use server::serve;
use crate::cli::SubCommand::*;
use log::error;

fn main() {
    logsy::set_echo(true);
    let args = cli::RunArgs::parse();
    let result = match args.cmd {
        Serve { src_paths, port } => Ok(serve(src_paths, port)),
        Download { url, dst_paths, auth, threads, dry_run } => run_client(&url, dst_paths, &auth, threads, dry_run),
    };
    if let Err(err) = result {
        error!("Operation failed: {:#}", err);
    }
}
