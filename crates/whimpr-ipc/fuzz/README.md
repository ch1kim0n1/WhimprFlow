# Fuzzing Targets for whimpr-ipc

This directory contains fuzzing targets for the IPC codec using libfuzzer-sys.

## Running the fuzzers

Install cargo-fuzz:
```bash
cargo install cargo-fuzz
```

Run the codec fuzzer:
```bash
cd crates/whimpr-ipc
cargo fuzz run fuzz_codec
```

Run the decode fuzzer:
```bash
cargo fuzz run fuzz_decode
```

## Targets

- `fuzz_codec`: Fuzzes the frame encoding/decoding path with round-trip consistency checks
- `fuzz_decode`: Fuzzes the JSON decoding path to ensure graceful handling of malformed input

## Continuous Fuzzing

To run fuzzers overnight or in CI:
```bash
cargo fuzz run fuzz_codec -- -max_total_time=3600
```