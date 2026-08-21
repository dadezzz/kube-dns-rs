#[derive(clap::Parser)]
pub struct Args {
    #[arg(short, long, default_value_t = "/etc/kube-dns-rs/config.yaml".to_string())]
    pub config: String,
}
