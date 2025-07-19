use clap::{ Parser, Subcommand, Args };

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
    Download(#[clap(flatten)] DownloadConfig),
}

#[derive(Args, Debug)]
pub struct DownloadConfig {
    pub url: String,
    pub dst_paths: Vec<String>,
    #[arg(long)]
    pub auth: String,
    #[arg(long, default_value_t=16)]
    pub threads: u16,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub group_by: Option<String>,
    #[arg(long)]
    pub group_by_preload: Option<String>,
}
