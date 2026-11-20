# Reviewed SSH dependency patches

cpTerm needs native Windows, Linux and macOS SSH/SFTP support. The locked upstream
`russh-sftp` 2.4.0 and `russh-util` 0.52.0 require `chrono`; newer releases still
retain it. These private patches remove that dependency without replacing the
SSH backend, cryptography, host-key validation or transfer implementation.

Sources came from the exact crates.io archives, verified against their original
`Cargo.lock` SHA-256 checksums before extraction. `provenance.json` records those
checksums, upstream revisions, changes and hashes of every retained file.
Upstream licenses and copyright notices are retained. The `russh-util` archive
declares Apache-2.0 but omits the license text; its copy of `LICENSE` supplies the
standard Apache-2.0 text also shipped by `russh-sftp`.

Changes:

- `russh-sftp`: use `std::time` for protocol epoch seconds, preserving the old
  flooring and wrapping behavior. Use the already locked `time` crate for UTC
  longname formatting. Remove unused examples, benchmarks and development
  dependencies from the private manifest; retain all library sources. Rename
  the deprecated atomic `fetch_update` call to its equivalent `try_update` name
  so current Rust's warning checks also pass for this local dependency.
- `russh-util`: remove the WebAssembly-only datetime dependency and use native
  `std::time::Instant`. This copy explicitly rejects WebAssembly, which cpTerm
  does not support. Native runtime behavior is unchanged.
- Published CI configuration, examples, benchmark files, original manifests
  and nested lockfiles are omitted because these copies are private library
  dependencies. No upstream security checks are being bypassed.

The audit checks file hashes and patch destinations before querying OSV under
each original crate name and version. All advisories block releases. A future
advisory about a patched area still needs review; it is never silently ignored.
The audit also rejects any reintroduction of `chrono` anywhere in `Cargo.lock`.

For updates, review the upstream changes and current advisories, verify the new
archive checksum, reapply only the required patches, and regenerate provenance
after review. Exercise timestamp regressions and the Linux loopback SFTP test.
Remove each patch when an upstream release can replace it without `chrono`.
