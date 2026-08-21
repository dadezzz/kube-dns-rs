use kube::CustomResourceExt;
use kube_dns_rs::kubernetes::crd::DnsRecord;

fn main() {
    println!("{}", yaml_serde::to_string(&DnsRecord::crd()).unwrap());
}
