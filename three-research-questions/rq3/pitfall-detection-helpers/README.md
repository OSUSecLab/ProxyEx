# Pitfall Detection Scripts

The helper scripts that facilitate the semi-automated detection of proxy-logic collisions, logic-logic collisions, and uninitialized proxies. 

## Prerequisite

- Reth archive node using (reth alpha version):
Reth's beta/stable version changes its database structure so some code will not work. 
Reth alpha version can be download [here](https://github.com/paradigmxyz/reth/releases/tag/v0.1.0-alpha.22), and you need to sync an archive node of Ethereum mainnet before using the scripts since our scripts replay historical transactions.
- Postgres database:
The output data of the scripts are saved into a postgres database.

## Configuration

The file `config.toml` defines some configurations used by the scripts, including:
- path to the datadir of reth archive node.
- connection url to postgres database.

## Description

Here are the entrypoint of scripts (rust main functions):
- Proxy-logic collision detection - filter proxies which has write-write conflicts between proxy contract and logic contract: `bin/replay/main.rs`
- Logic-logic collision detection - replay transactions in newer versions of logic contracts: `bin/regression/main.rs`
- Uninitialized proxy detection - collect different calldata to initialize contracts/check if a proxy is uninitialized after deployment using front-run: `bin/uninitialized/main.rs`
