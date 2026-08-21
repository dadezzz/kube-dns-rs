use kube::CustomResourceExt;
use kube_dns_rs::kubernetes::crd::DnsRecord;

fn main() {
    println!("{}", serde_yaml::to_string(&DnsRecord::crd()).unwrap());
}
