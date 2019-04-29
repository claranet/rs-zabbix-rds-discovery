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
use rusoto_rds::{DescribeDBInstancesMessage, ListTagsForResourceMessage, Rds, RdsClient, Tag};
use rusoto_sts::{StsAssumeRoleSessionCredentialsProvider, StsClient};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct ArgTag {
    key: String,
    value: String,
}

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

    let auto_refreshing_provider =
        AutoRefreshingProvider::new(provider).expect("Could not get auto refreshing STS provider");

    // Get RDS Instances
    // TODO: implement pagination, we are currenty limited to 100 RDS instances.
    //       This should be enough for now.
    let rds = RdsClient::new_with(
        HttpClient::new().expect("Could not get HTTP client"),
        auto_refreshing_provider,
        region.clone(),
    );

    let ddbi_message = DescribeDBInstancesMessage::default();
    let dbi_message = rds
        .describe_db_instances(ddbi_message)
        .sync()
        .expect("Could not describe DB Instances");

    // Loop over RDS Instances
    for db in dbi_message
        .db_instances
        .unwrap_or_else(Vec::new)
        .into_iter()
    {
        if matches.is_present("tags") {
            // Parse tags
            let tags_json = matches.value_of("tags").unwrap();
            let tags: Vec<ArgTag> =
                serde_json::from_str(tags_json).expect("Could not parse tags JSON");

            // Get instance tags
            let ltfr_message = ListTagsForResourceMessage {
                resource_name: db.db_instance_arn.clone().unwrap(),
                filters: None,
            };
            let tl_message = rds
                .list_tags_for_resource(ltfr_message)
                .sync()
                .expect(&format!(
                    "Could not list Tags for instance {}",
                    db.db_instance_arn.unwrap()
                ));
            let tag_list = tl_message.tag_list.unwrap_or_else(Vec::new);

            // Check for matching tags
            for tag in tags.into_iter() {
                if tag_list.contains(&Tag {
                    key: Some(tag.key.to_owned()),
                    value: Some(tag.value.to_owned()),
                }) {
                    // Add instance to output and break on match
                    data.push(DiscoveryEntry {
                        db_instance_identifier: db.db_instance_identifier.unwrap(),
                        address: db.endpoint.clone().unwrap().address.unwrap(),
                        port: db.endpoint.unwrap().port.unwrap(),
                    });
                    break;
                }
            }
        } else {
            // Or just add instance to instance list
            data.push(DiscoveryEntry {
                db_instance_identifier: db.db_instance_identifier.unwrap(),
                address: db.endpoint.clone().unwrap().address.unwrap(),
                port: db.endpoint.unwrap().port.unwrap(),
            });
        };
    }

    // Print Zabbix Data
    println!(
        "{}",
        serde_json::to_string_pretty(&DiscoveryData { data }).expect("Could not serialize Output")
    );
}
