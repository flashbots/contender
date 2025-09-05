# Placeholders in scenarios

Placeholders are used to substitute contract addresses, or custom values defined in `[env]`.

- In `[[create]]`: allowed in `bytecode` & `from`.
- In `[[setup]]`/`[[spam]]`: allowed in `to`, `args`, `from`, and `value`.
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

Custom variables:
```toml
[env]
initialSupply = "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
customSender = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

[[create]]
name = "testToken"
from = "{customSender}"
bytecode = "0x60806040..."
signature = "(uint256 _initialSupply)"
args = ["{initialSupply}"]
```

`{_sender}` is a special placeholder that injects the sender's address:

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
