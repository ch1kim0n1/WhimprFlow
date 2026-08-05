# Fuzzing Targets for whimpr-core

This directory contains fuzzing targets for the license parser using libfuzzer-sys.

## Running the fuzzers

Install cargo-fuzz:
```bash
cargo install cargo-fuzz
```

Run the license parse fuzzer:
```bash
cd crates/whimpr-core
cargo fuzz run fuzz_license_parse
```

Run the license verify fuzzer:
```bash
cargo fuzz run fuzz_license_verify
```

## Targets

- `fuzz_license_parse`: Fuzzes the full license key parsing (format: WF1.<base64url(json)>.<base64url(sig)))
- `fuzz_license_verify`: Fuzzes the inner JSON payload deserialization

## Continuous Fuzzing

To run fuzzers overnight or in CI:
```bash
cargo fuzz run fuzz_license_parse -- -max_total_time=3600
```