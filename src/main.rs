// rs-zabbix-rds-discovery
// Copyright (C) 2019  Claranet France

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use clap::{load_yaml, App};
use rusoto_core::{HttpClient, Region};
use rusoto_credential::AutoRefreshingProvider;
use rusoto_rds::{
    DBInstanceMessage, DescribeDBInstancesMessage, ListTagsForResourceMessage, Rds, RdsClient, Tag,
};
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

fn get_rds_instances(client: &RdsClient) -> DBInstanceMessage {
    // Get RDS Instances
    // TODO: implement pagination, we are currenty limited to 100 RDS instances.
    //       This should be enough for now.
    let ddbi_message = DescribeDBInstancesMessage::default();
    client
        .describe_db_instances(ddbi_message)
        .sync()
        .expect("Could not describe DB Instances")
}

fn discover_rds_instances(
    client: &RdsClient,
    db_instances: DBInstanceMessage,
    tags: Option<Vec<ArgTag>>,
) -> Vec<DiscoveryEntry> {
    let mut data = vec![];

    // Loop over RDS Instances
    for db in db_instances
        .db_instances
        .unwrap_or_else(Vec::new)
        .into_iter()
    {
        if let Some(ref tags) = tags {
            // Get instance tags
            let ltfr_message = ListTagsForResourceMessage {
                resource_name: db.db_instance_arn.clone().unwrap(),
                filters: None,
            };
            let tl_message = client
                .list_tags_for_resource(ltfr_message)
                .sync()
                .unwrap_or_else(|_| {
                    panic!(
                        "Could not list Tags for instance {}",
                        db.clone().db_instance_arn.unwrap()
                    )
                });
            let tag_list = tl_message.tag_list.unwrap_or_else(Vec::new);

            // Check for matching tags
            for tag in tags.iter() {
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

    data
}

fn main() {
    let mut tags = None::<Vec<ArgTag>>;

    // Parse CLI parameters
    let yaml = load_yaml!("clap.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let region = matches
        .value_of("region")
        .unwrap()
        .parse::<Region>()
        .expect("Could not parse region");
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

    // Get RDS Client
    let rds = RdsClient::new_with(
        HttpClient::new().expect("Could not get HTTP client"),
        auto_refreshing_provider,
        region,
    );

    // Should we match some tags ?
    if matches.is_present("tags") {
        // Parse tags
        let tags_json = matches.value_of("tags").unwrap();
        tags = serde_json::from_str(tags_json).expect("Could not parse tags JSON");
    };

    // Discover instances
    let rds_instances = get_rds_instances(&rds);
    let data = discover_rds_instances(&rds, rds_instances, tags);

    // Print Zabbix Data
    println!(
        "{}",
        serde_json::to_string_pretty(&DiscoveryData { data }).expect("Could not serialize Output")
    );
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use assert_json_diff::assert_json_eq;
    use rusoto_mock::*;
    use rusoto_rds::RdsClient;

    #[test]
    fn return_all_instances() {
        let rds = RdsClient::new_with(
            MockRequestDispatcher::default().with_body(&MockResponseReader::read_response(
                "test-data",
                "rds-success-all-instances.xml",
            )),
            MockCredentialsProvider,
            Default::default(),
        );

        let rds_instances = get_rds_instances(&rds);
        let data = discover_rds_instances(&rds, rds_instances, None);
        let json = serde_json::to_value(DiscoveryData { data }).unwrap();

        let file = fs::File::open("test-data/rds-success-all-instances_expected.json").unwrap();
        let expected_json = serde_json::from_reader(file).unwrap();

        assert_json_eq!(json, expected_json)
    }

    #[test]
    fn return_match() {
        let rds_client_instances = RdsClient::new_with(
            MockRequestDispatcher::default().with_body(&MockResponseReader::read_response(
                "test-data",
                "rds-success-match_DescribDBInstances.xml",
            )),
            MockCredentialsProvider,
            Default::default(),
        );

        let rds_client_tags = RdsClient::new_with(
            MockRequestDispatcher::default().with_body(&MockResponseReader::read_response(
                "test-data",
                "rds-success-match_ListTagsForResource.xml",
            )),
            MockCredentialsProvider,
            Default::default(),
        );

        let tags = vec![ArgTag {
            key: "project".to_string(),
            value: "foo".to_string(),
        }];

        let rds_instances = get_rds_instances(&rds_client_instances);
        let data = discover_rds_instances(&rds_client_tags, rds_instances, Some(tags));
        let json = serde_json::to_value(DiscoveryData { data }).unwrap();

        let file = fs::File::open("test-data/rds-success-match_expected.json").unwrap();
        let expected_json = serde_json::from_reader(file).unwrap();

        assert_json_eq!(json, expected_json)
    }

    #[test]
    fn return_no_match() {
        let rds_client_instances = RdsClient::new_with(
            MockRequestDispatcher::default().with_body(&MockResponseReader::read_response(
                "test-data",
                "rds-success-no-match_DescribDBInstances.xml",
            )),
            MockCredentialsProvider,
            Default::default(),
        );

        let rds_client_tags = RdsClient::new_with(
            MockRequestDispatcher::default().with_body(&MockResponseReader::read_response(
                "test-data",
                "rds-success-no-match_ListTagsForResource.xml",
            )),
            MockCredentialsProvider,
            Default::default(),
        );

        let tags = vec![ArgTag {
            key: "project".to_string(),
            value: "bar".to_string(),
        }];

        let rds_instances = get_rds_instances(&rds_client_instances);
        let data = discover_rds_instances(&rds_client_tags, rds_instances, Some(tags));
        let json = serde_json::to_value(DiscoveryData { data }).unwrap();

        let file = fs::File::open("test-data/rds-success-no-match_expected.json").unwrap();
        let expected_json = serde_json::from_reader(file).unwrap();

        assert_json_eq!(json, expected_json)
    }
}
