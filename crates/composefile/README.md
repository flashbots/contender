# Compose File

Contender Compose file is a YAML file that allows us to 

1. Set up multiple scenarios by running `setup` command for multiple testfiles/scenario files.
2. Run these scenarios using `spam` command for multiple scenarios at a time.
3. Has a concept of `stages`, which allows you to execute a list of `spam` commands parallely.

## Get started


```yaml
setup:
  simpler:
    testfile: ./scenarios/simpler.toml
    min_balance: 12.1
  uniV2:
    testfile: ./scenarios/uniV2.toml
    rpc_url: http://localhost:8545 # Optional
    min_balance: "11"
    env:
      - Key1=Valu1
      - Key2=Valu2
    private_keys: # Optional (these Private keys are from Anvil)
      - 0xABCD
      - 0xPQRS
    tx_type: eip1559  # Optional

spam:
  stages:
    warmup:
      - testfile: ./scenarios/simpler.toml
        tps: 3
        duration: 3
        loop: 2
      - testfile: ./scenarios/simpler.toml
        tps: 1
        duration: 3
        loop: 2
    medium:
      - testfile: ./scenarios/simpler.toml
        tps: 14
        duration: 3
        loop: 2
```

This file first runs the following commands parallely

```
contender setup ./scenarios/simpler.toml --min_balance 12.1
contender setup ./scenarios/uniV2.toml --rpc-url http://localhost:8545 --min-balance 11 --env Key1=Valu1 --env Key2=Valu2 -p 0xABCD -p 0xPQRS --tx-type eip1559
```

And then the following spam commands

```bash
# Run these 2 parallely first, since they are in the same stage
contender spam --txs-per-second 3 -d 3 -l 2 ./scenarios/simpler.toml 
contender spam --txs-per-second 1 -d 3 -l 2 ./scenarios/simpler.toml 

# Then run this (medium)
contender spam --txs-per-second 14 -d 3 -l 2 ./scenarios/simpler.toml 

```

## Guidelines to write a composefile

An example `contender-compose.yml` file

```yml
# This `setup` block is optional, if there's no need to deploy 
setup:
  # We can run multiple setups simultaneously
  <SCENARIO_NAME_1>:
    testfile: ./path/to/scenario.toml # Required field, the path to the scenario.toml file.
    # Can be a remote url (scenario:simpler.toml, scenario:uniV2.toml) or a relative path to the toml file
    rpc_url: https://rpc_some_chain.com  # Optional, default value = http://localhost:8545
    env:  # Optional, an array of `env_variable=env_value`
      - <VARIABLE_1>=<VALUE_1>
      - <VARIABLE_2>=<VALUE_2>

    private_keys: # Optional field, an Array of private keys
      - 0xABCD...
      - 0xPQRS
    min_balance: 10 # Optional, default value = 0.01
    # Can be an integer, a float, or a numeric string ("101", "20.01" etc)
    tx_type: eip1559 # Optional. Value could be one of "eip1559" or "legacy". Default value = eip1559
  <SCENARIO_NAME_2>:
    ...
    # Can have multiple different scenarios with unique names

# This key is required
spam:
  # Required Key
  stages:
    # Unique Stage name keys (can't be empty).
    # The spam items within each stage are run parallely 
    <STAGE_NAME>:
      - testfile: ./scenarios/simpler.toml # Required field, the path to the scenario.toml file.
        # Can be a remote url (scenario:simpler.toml, scenario:uniV2.toml) or a relative path to the toml file
        tps: 3 # Txs_per_second. Required field. (Either tps or tpb required)
        tpb: 5 # Txs_per_block. Required ONLY if tps not set. Only one of tps or tpb can exist
        duration: 3 # Optional. Number of seconds the spam runs for. Defaul value=1 (second)
        loop: 2 # Optional. Number of times to run spam for. (loop:2 runs simpler.toml scenario with 3 txs_per_second for 3 seconds, for 2 iterations) Default value=1
        builder_url: http://builder_url.com # Optional, if you want a block builder (no block builder url needed by default)
        timeout_secs: 5 # Optional. Timeout for pending transactions in the mempool. Default value = 12
        rpc_url: https://rpc_some_chain.com  # Optional, default value = http://localhost:8545
        env:  # Optional, an array of `env_variable=env_value`
          - <VARIABLE_1>=<VALUE_1>
          - <VARIABLE_2>=<VALUE_2>

        private_keys: # Optional field, an Array of private keys
          - 0xABCD...
          - 0xPQRS
        min_balance: 10 # Optional, default value = 0.01
        # Can be an integer, a float, or a numeric string ("101", "20.01" etc)
        tx_type: eip1559 # Optional. Value could be one of "eip1559" or "legacy". Default value = eip1559

      # Some example spam arguments that are valid
      - testfile: ./scenarios/uniV2.toml
        tps: 1
        duration: 3
        loop: 2
        min_balance: 20
      - testfile: ./scenarios/uniV3.toml
        tpb: 5
    <STAGE_NAME_2>:
      - ...(another spam object)
      - ...

```

We can run this file with the command 
`contender compose` and it'll run the `contender-compose.yml`.

If the name of this yml file is different to `contender-compose.yml`, use the flag `-f/--filename` as

`contender compose -f <COMPOSE_FILE_PATH>.yml`
