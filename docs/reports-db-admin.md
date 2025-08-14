# Reports, DB, and Admin

## Reports

Contender can generate HTML reports highlighting your target chain's performance characteristics.

```bash
# latest run (opens in browser)
contender report

# include 2 previous runs
contender report -p 2

# explicit range (inclusive, run #203 and the 3 runs before that)
contender report -i 203 -p 3
```

## Database ops

```bash
contender db export ./backup.db
contender db import ./backup.db
contender db reset
contender db drop
```

## Admin conveniences

```bash
# derive accounts from a from_pool
contender admin accounts --from-pool "spammers" -n 100

# view the local seed
contender admin seed

# last run id
contender admin latest-run-id
```
