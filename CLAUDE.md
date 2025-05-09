# Claude Notes

## Testing

Use `cargo nextest` to run tests instead of the standard `cargo test` command.

```bash
cargo nextest run
```

Benefits of nextest:
- Faster test execution with parallel testing
- Better output formatting
- More detailed failure reporting

## Searching code

Use `rg` (ripgrep) for fast code searches instead of `grep` or `find`.