# Placeholders in scenarios

Placeholders substitute contract addresses, sender address, or `[env]` values.

- In `[[create]]`: allowed in `bytecode` only.
- In `[[setup]]`/`[[spam]]`: allowed in `to`, `args`, and `value`.
- `{_sender}` resolves to the `from` address at runtime.

### Examples

Contract address placeholders:
```toml
[[create]] # -> {weth}
name = "weth"
# ...

[[create]] # -> {testToken}
name = "testToken"

[[setup]]
kind = "univ2_create_pair"
to = "{uniV2Factory}"

signature = "function createPair(address tokenA, address tokenB) external returns (address pair)"
args = ["{weth}", "{testToken}"]
```

Custom variable:
```toml
[env]
initialSupply = "00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"

[[create]]
name = "testToken"

bytecode = "0x60806040...{initialSupply}"
```

Sender placeholder:
```toml
[[setup]]
kind = "admin_univ2_add_liquidity_weth-testToken"
to = "{uniRouterV2}"

signature = "addLiquidity(address,address,uint,uint,uint,uint,address,uint) returns (uint,uint,uint)"
args = [
  "{weth}", "{testToken}",
  "2500000000000000000",
  "50000000000000000000000",
  "100000000000000",
  "5000000000000000",
  "{_sender}",
  "10000000000000"
]
```
