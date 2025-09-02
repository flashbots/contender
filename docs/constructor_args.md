# Constructor args in [[create]]

Contender can ABI-encode constructor args and automatically append them to the `bytecode` during contract creation. This removes the need for manual zero-padding or bytecode manipulation.

Usage:

```toml
[[create]]
name = "MyToken"
from_pool = "admin"
signature = "(uint256 initialSupply)"   # or "constructor(uint256 initialSupply)"
args = ["{initialSupply}"]
bytecode = "0x6080..."  # compiled runtime bytecode
```

Notes:
- Supported signature formats: "constructor(type1,type2,...)" or "(type1,type2,...)".
- `args` accept decimal strings or 0x-hex strings and support placeholders like `{weth}`, `{uniV2Factory}`, and `{_sender}`.
- Placeholders are resolved before encoding.
- The 4-byte selector from the signature is removed; only encoded constructor args are appended to `bytecode`.

