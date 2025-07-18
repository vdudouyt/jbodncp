mod filelist;
mod cli;
mod client;
mod server;
mod jbod;

use clap::Parser;
use client::run_client;
use server::serve;
use crate::cli::SubCommand::*;

fn main() {
    logsy::set_echo(true);
    let args = cli::RunArgs::parse();
    match args.cmd {
        Serve { src_paths, port } => serve(src_paths, port),
        Download { url, dst_paths, auth, threads } => run_client(&url, dst_paths, &auth, threads).unwrap(),
    }
}
