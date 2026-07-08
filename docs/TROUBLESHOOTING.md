# Troubleshooting

## Common Issues

### Issue: Build fails
- Solution: Update Rust toolchain
  ```bash
  rustup update
  ```

### Issue: Tests fail
- Solution: Run with logging
  ```bash
  RUST_LOG=debug cargo test
  ```
