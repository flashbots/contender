# Architecture

Modular components:
- **Generators** build transaction requests from config
- **Spammers** schedule and send txs
- **Callbacks** handle responses and logging
- **Database** persists deployments and test data
- **CLI** orchestrates runs and reporting

```mermaid
graph TD
  A[TOML Config File] -->|Parsed by| B[TestConfig]
  B -->|Configures| C[Generator]
  C -->|Produces| D[Transaction Requests]
  D -->|Fed to| E[Spammer]
  E -->|Sends txs| F[Ethereum Network]
  F -->|Responses| G[Callback Handler]
  G -->|Logs results| H[Database]

  I[CLI] -->|Reads| A
  I -->|Controls| E
  I -->|Queries| H

  H -->|Data for| J[Report Generator]
```
