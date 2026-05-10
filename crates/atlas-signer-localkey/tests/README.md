# atlas-signer-localkey integration tests

Integration tests for [`atlas-signer-localkey`](..). Each construction path gets its own file so a failure narrows the problem domain quickly.

## Layout

```text
              tests/
              │
              ├── raw_key.rs        from_bytes — happy path
              │                     SigningRequest → SignatureOnly
              │                     signature shape (65 bytes, v ∈ {0,1})
              │                     unsupported curve / payload kind
              │
              ├── keystore.rs       from_keystore (Web3 secret-storage)
              │                     pbkdf2 + scrypt JSON decryption
              │                     known-good fixture
              │
              ├── hd.rs             from_mnemonic — happy path
              │                     reads m/44'/60'/0'/0/0 from the
              │                     official chain registry, derives
              │                     0x9858EfFD…aEda94 from the BIP-39
              │                     standard test mnemonic
              │
              └── error_paths.rs    construction + signing failure modes
                                    invalid mnemonic / path / keystore
                                    multi-account derivation differs
```

## Why split by construction path?

```text
                  raw bytes ─────► from_bytes ────────┐
                                                      │
                  Web3 JSON ─────► from_keystore ─────┤
                                                      ├─► PrivateKeySigner
                  mnemonic ──────► from_mnemonic ─────┘     │
                                                            ▼
                                                      sign(req) returns
                                                      SigningResponse::
                                                        SignatureOnly
```

Each path has its own surface area (pbkdf2, scrypt, BIP-39, BIP-32 derivation) and its own failure modes. Keeping them in separate files means a single test failure points at one component, not at the union of three.

## Registry-driven HD path

`hd.rs` doesn't hardcode `"m/44'/60'/0'/0/0"`. It loads the official chain registry, looks up the EVM `Chain` entry, and pulls the path from `Chain.default_derivation_path`. This is the pattern any wallet integration should follow — the registry owns the path, signers consume it.

## Running

```bash
cargo test -p atlas-signer-localkey

# One file
cargo test -p atlas-signer-localkey --test hd
```

## Coverage gaps (intentional)

Two error branches need a hostile keystore JSON to fire — the `decrypted_key.len() != 32` guards in `from_keystore` and `from_keystore_path`. `eth-keystore`'s well-formed-but-bad-length corpus is empty in practice; the guards exist as belt-and-braces and stay uncovered. This brings the crate's coverage to ~83% without leaving a real-world path untested.
