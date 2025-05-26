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
