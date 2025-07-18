use clap::{ Parser, Subcommand };

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct RunArgs {
    #[clap(subcommand)]
    pub cmd: SubCommand,
}

#[derive(Subcommand, Debug)]
pub enum SubCommand {
    Serve {
        src_paths: Vec<String>,
        #[arg(long, default_value_t=3000)]
        port: u16,
    },
    Download {
        url: String,
        dst_paths: Vec<String>,
        #[arg(long)]
        auth: String,
        #[arg(long, default_value_t=16)]
        threads: u16
    },
}
