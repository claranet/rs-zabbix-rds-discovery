# rs-zabbix-rds-discovery

Discover RDS DB Instances and return them in a format usable by Zabbix Discovery.

## Compiling from Source

```
git clone https://github.com/claranet/rs-zabbix-rds-discovery.git
cd rs-zabbix-rds-discovery
cargo build --release
```

## Usage

```
./rs-zabbix-rds-discovery --help
```

## Features

Both `native-tls` ans `rustls` are supported.

If you need to use `rustls` :

```
cargo build --release --no-default-features --features rustls
```
