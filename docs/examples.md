# Example invocations

*Before you do anything, run `contender spam -h` to familiarize yourself with all the possible usages of the spam command.*

---

Fill blocks (zero-config scenario):
```bash
contender spam --tps 50 -r $RPC_URL fill-block
```

Send per block (not per second):
```bash
contender spam --tpb 50 -r $RPC_URL fill-block
```

Send 10 batches of transactions before checking for receipts:
```bash
contender spam --tps 50 -d 10 -r $RPC_URL fill-block
```

Fund agents from your own key:
```bash
contender spam --tps 50 -d 10 -r $RPC_URL -p $PRIVATE_KEY fill-block
```

Custom scenario — setup then spam:
```bash
contender setup scenario:stress.toml -r $RPC_URL
contender spam  scenario:stress.toml -r $RPC_URL --tps 10 -d 3
```

Funding spammers with `-p`:
```bash
contender spam scenario:stress.toml -r $RPC_URL --tps 10 -d 3 -p $PRV_KEY
```

Agent-account math:

- `spam --min-balance` sets the minimum balance a spammer account can hold
  - that amount will be sent when a spammer starts and the balance of the account is below it
- `spam -a` specifies the number of accounts with which to spam. The default amount is 10.
- if you specify `spam --min-balance 1eth -a 50 -p $PRV_KEY` your account must hold at least 50 ETH

Reports:
```bash
# latest run
contender report

# include 2 previous runs (total 3)
contender report -p 2

# explicit range (inclusive)
contender report -i 203 -p 3
```

`[env]` overrides:
```toml
# example.toml (snippet)
[[spam]]
[spam.tx]
to = "{testAddr}"

signature = "call()"
args = []
```

```bash
contender spam ./example.toml --tps 10 -e testAddr=0x0000000000000000000000000000000000000013
```
