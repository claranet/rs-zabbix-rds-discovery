#[macro_use(load_yaml)]
extern crate clap;
extern crate rusoto_core;
extern crate rusoto_credential;
extern crate rusoto_rds;
extern crate rusoto_sts;
extern crate serde;
extern crate serde_json;

use clap::App;
use rusoto_core::{HttpClient, Region};
use rusoto_credential::AutoRefreshingProvider;
use rusoto_rds::{DescribeDBInstancesMessage, Rds, RdsClient};
use rusoto_sts::{StsAssumeRoleSessionCredentialsProvider, StsClient};
use serde::Serialize;

#[derive(Serialize)]
struct DiscoveryData {
    data: Vec<DiscoveryEntry>,
}

#[derive(Serialize)]
struct DiscoveryEntry {
    #[serde(rename = "{#DB}")]
    db_instance_identifier: String,
    #[serde(rename = "{#DB_ENDPOINT}")]
    address: String,
    #[serde(rename = "{#DB_PORT}")]
    port: i64,
}

fn main() {
    let mut data = vec![];

    // Parse CLI parameters
    let yaml = load_yaml!("clap.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let region = matches
        .value_of("region")
        .unwrap()
        .parse::<Region>()
        .unwrap();
    let role = matches.value_of("role").unwrap();

    // Get auto-refreshing credentials from STS
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

    let auto_refreshing_provider = AutoRefreshingProvider::new(provider).unwrap();

    // Get RDS Instances
    // TODO: implement pagination, we are currenty limited to 100 RDS instances.
    //       This should be enough for now.
    let rds = RdsClient::new_with(
        HttpClient::new().unwrap(),
        auto_refreshing_provider,
        region.clone(),
    );

    let ddbi_message = DescribeDBInstancesMessage::default();
    let dbi_message = rds.describe_db_instances(ddbi_message).sync().unwrap();


    // Loop over RDS Instances
    for db in dbi_message
        .db_instances
        .unwrap_or_else(Vec::new)
        .into_iter()
    {
        data.push(DiscoveryEntry {
            db_instance_identifier: db.db_instance_identifier.unwrap(),
            address: db.endpoint.clone().unwrap().address.unwrap(),
            port: db.endpoint.unwrap().port.unwrap(),
        });
    }

    // Print Zabbix Data
    println!(
        "{}",
        serde_json::to_string_pretty(&DiscoveryData { data }).unwrap()
    );
}
