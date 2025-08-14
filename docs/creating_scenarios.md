# contender scenario config

*a walkthrough for creating new Contender scenarios with basic templates.*

---

To create new scenarios for Contender to run, make a new TOML file:

```bash
touch MyScenario.toml
```

## env variables & placeholders

Contender scenario files make use of a templating engine that allows us to define variables in the file that can be used throughout the file. This can be used for contract deployments and custom variables.

To set a custom variable, create an `[env]` section at the top of the file:

```toml
[env]
<varName> = ""
```

For example, the [UniV2 scenario](../scenarios/uniV2.toml#L5-L6) defines an `initialSupply` variable to be used when minting tokens:

```toml
[env]
initialSupply = "00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
```

You may create as many variables as you want.

Following that declaration in the TOML file, you can reference the variable in several places using the `{placeholder}` syntax.

In the [UniV2 example](../scenarios/uniV2.toml#L14-L17), the `initialSupply` variable is used to mint tokens during a token deployment by passing the variable as a constructor argument to the bytecode:

```toml
[[create]]
name = "testToken"
from_pool = "admin"
bytecode = "0x608060...0033{initialSupply}"
```

## defining a contract deployment

Copy in the following boilerplate:

```toml
[[create]]
bytecode = ""
name = ""
from_pool = "admin"
```

This is how contender defines a contract deployment.

- The `bytecode` field contains the data we need to deploy the contract to the chain.
- The `name` field gives the contract a name that can be used in later steps, where we define transactions to send to our contracts.
- The `from_pool` field generates a new signer that can be referenced by that name, so that we don’t have to hard-code a sender, allowing us to write scenarios that anyone can run.

Start by assigning the name of your contract to the `name` field.

To get bytecode for your contract using [`forge`](https://getfoundry.sh/) & [`jq`](https://jqlang.github.io/jq/):

```bash
# in your forge project directory
forge build
# replace "SpamMe" with your own contract
cat out/SpamMe.sol/SpamMe.json | jq .bytecode.object
```

Copy-and-paste the output of that last command into the `bytecode` field.

When you’re done, it should look something like this:

```toml
[[create]]
bytecode = "0x608060405..." # truncated; it's usually very long
name = "SpamMe2"
from_pool = "admin"
```

Add as many of these `[[create]]` steps as you need. They will be deployed in order.

### passing constructor args

To provide constructor args, you can inject deployed contract addresses by their name in latter contract deployments using the `{placeholder}` syntax.

For example, in the [builtin UniV2 scenario](../scenarios/uniV2.toml#L34), we use several placeholders. Here’s a snippet:

```toml
[[create]]
name = "uniRouterV2"
from_pool = "admin"
# requires {univ2Factory} and {weth}
bytecode = "0x60c06040...060033000000000000000000000000{uniV2Factory}000000000000000000000000{weth}"
```

> 💡Note that we have to manually insert leading zeros to pad the address to 32 bytes.
> 

In this example, `{uniV2Factory}` and `{weth}` were deployed before `{uniRouterV2}`, so we’re able to use them as constructor args.

This [may change soon](https://github.com/flashbots/contender/issues/105).

## defining setup steps

Contender can run one-time transactions after deploying your contracts. This lets you set the base state for your scenario before spamming. In the UniV2 scenario, we use this to deposit ETH for WETH, mint tokens, launch trading pairs, etc.

Copy in this boilerplate definition for a `[[setup]]` step:

```toml
[[setup]]
kind = ""
to = ""
from_pool = "admin"
signature = ""
args = [
]
value = ""
```

You’ll notice some new fields:

- `kind` just gives the transaction a label, which can make debugging easier
- `to` is the recipient address.
    - You may use a `{placeholder}` here.
- `signature` is a solidity function signature; the function called by this transaction.
- `args` are the arguments to the function; they can be decimal strings or hex strings.
    - You may use a `{placeholder}` here.
- `value` is how much ether to send with the transaction.
    - May be passed as a decimal string or hex string.
    - You may use a `{placeholder}` here.

Here’s a snippet from the [UniV2 scenario](../scenarios/uniV2.toml#L43-L62), where we deposit ETH to get WETH, then create a token pair on UniV2:

```toml
# get 10 WETH
[[setup]]
kind = "admin_weth_deposit"
to = "{weth}"
from_pool = "admin"
signature = "function deposit() public payable"
value = "10000000000000000000"

# create TOKEN1/WETH pair
[[setup]]
kind = "univ2_create_pair_token1-weth"
to = "{uniV2Factory}"
from_pool = "admin"
signature = "function createPair(address tokenA, address tokenB) external returns (address pair)"
args = [
     "{weth}",
     "{testToken}"
]
```

Add as many setup steps as you need, then once you’re confident your scenario’s base state is constructed, you’re ready to define some spam steps.

## defining spam steps

Spam steps have the same structure as setup steps, but they’re nested in a wrapper. Copy this template into your scenario file:

```toml
[[spam]]

[spam.tx]
to = ""
from_pool = ""
signature = ""
args = []
```

> 💡 Notice that we have a new `[spam.tx]` directive under `[[spam]]` . This allows us to differentiate between mempool txs and bundles (we’ll cover bundles later).

One important thing to consider when writing these is the `from_pool` definition. You probably don’t want to spam with the “admin” pool (though it’s your choice), so we advise you use a different `from_pool` name for your spam definitions.

Here’s an example from the builtin [mempool](../scenarios/mempool.toml) scenario:

```toml
[[spam]]

[spam.tx]
to = "{SpamMe2}"
from_pool = "bluepool"
signature = "consumeGas(uint256 gasAmount)"
args = ["1350000"]
```

> 💡 You may want to define different `from_pool` definitions for different kinds of transactions to logically group your agents, which will make your orderflow easier to reason about.

### optional gas limit & sending reverting txs

You have the option to set `gas_limit` to skip gas estimation. This also enables reverting transactions to be sent.

```toml
[[spam]]

[spam.tx]
to = "{SpamMe2}"
from_pool = "bluepool"
signature = "consumeGas(uint256 gasAmount)"
args = ["1350000"]
gas_limit = 1350000
```

### sending bundles

The `[spam.tx]` directive sends a mempool transaction using `eth_sendRawTransaction`, but Contender also supports bundles.

To send a bundle, use `[[spam.bundle.tx]]` instead of `[spam.tx]`. The double-brackets indicate that we can specify it multiple times; each `[[spam.bundle.tx]]` under a single `[[spam]]` directive represents a transaction in a bundle.

The following snippet specifies a bundle with two transactions. The first one consumes some gas, then the second one pays a tip directly to the block builder to get included faster:

```toml
# spam bundle
[[spam]]

[[spam.bundle.tx]]
to = "{SpamMe}"
from_pool = "bluepool"
signature = "consumeGas(uint256 gasAmount)"
args = ["51000"]
fuzz = [{ param = "gasAmount", min = "22000", max = "69000" }]

[[spam.bundle.tx]]
to = "{SpamMe}"
from_pool = "bluepool"
signature = "tipCoinbase()"
value = "10000000000000000"
```

### fuzzing arguments

Spam steps allow you to define a `fuzz` parameter that generates pseudo-random values for your function call arguments.

Here’s an example from the [mempool](../scenarios/mempool.toml) scenario:

```toml
[[spam]]

[spam.tx]
to = "{SpamMe2}"
from_pool = "redpool"
signature = "consumeGas(uint256 gasAmount)"
args = ["3000000"]
fuzz = [{ param = "gasAmount", min = "1000000", max = "3000000" }]
```

The `param` field picks out the argument to inject by the name given in `signature`. If your signature doesn’t include argument names, you can make up your own; fuzzing arguments requires named params.

> 💡 Note that we require `args` to be specified, even when `fuzz` will replace its value. This may change.

## run it

Once your scenario config is complete, pass it to contender:

```bash
contender setup ./MyScenario.toml $RPC_URL --min-balance 0.25
contender spam ./MyScenario.toml $RPC_URL --tps 10 -d 3 -p $PRV_KEY --min-balance 0.05
```
