# Native desktop integration with current main

Parents: native test branch a4aa309 and main 6d57102.

The merge preserves native Remote lifecycle and asynchronous scan progress,
plus main's telemetry, support tiers and mobile billing. It removes the obsolete
mobile limited_writer module declaration and restores the direct RSA dependency
needed by Play purchase verification. Platform-tools remains the shipping default.

Completed before packaging: desktop and mobile npm check (0 errors/warnings)
and npm build; exact mobile command-matrix coverage (87 unique entries).
Independent source review identified the two compile integration issues above;
both are corrected. Final Rust validation and regenerated combined-feature
captures are pending. Current desktop gallery files are from main, not newly
captured evidence. No real-device operations, native release or Play acceptance
is claimed. Existing privacy, pricing, free-undo and hardware gates remain separate.
