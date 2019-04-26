#[macro_use(load_yaml)]
extern crate clap;
extern crate rusoto_core;
extern crate rusoto_rds;
extern crate rusoto_sts;

use clap::App;
use rusoto_core::{HttpClient, Region};
use rusoto_rds::{DescribeDBInstancesMessage, Rds, RdsClient};
use rusoto_sts::{StsAssumeRoleSessionCredentialsProvider, StsClient};

fn main() {
    // Parse CLI parameters
    let yaml = load_yaml!("clap.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let region = matches
        .value_of("region")
        .unwrap()
        .parse::<Region>()
        .unwrap();
    let role = matches.value_of("role").unwrap();

    // Get credentials from STS
    let sts = StsClient::new(region.clone());

    let provider = StsAssumeRoleSessionCredentialsProvider::new(
        sts,
        role.to_owned(),
        "zabbix-discovery".to_owned(),
        None,
        None,
        None,
        None,
    );

    // Get RDS Instances
    let rds = RdsClient::new_with(HttpClient::new().unwrap(), provider, region.clone());

    let ddbi_message = DescribeDBInstancesMessage::default();
    let dbi_message = rds.describe_db_instances(ddbi_message).sync();

    // see if it works
    println!("{:?}", dbi_message);
}
