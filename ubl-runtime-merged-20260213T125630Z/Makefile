.PHONY: build test run fmt clippy conformance

build:
	cargo build --workspace

test:
	cargo test --workspace

run:
	RUST_LOG=info cargo run -p ubl_tdln_svc

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

# sanity conformance: determinism + binding D8
conformance:
	cargo test -p ubl_tdln -- --nocapture


.PHONY: docs validate-wa-wf

docs:
	@echo "Formatting/validating runtime docs..."
	@test -f docs/runtime-ghost-write-ahead.md && echo "✓ ghost/write-ahead docs present"

validate-wa-wf:
	@echo "Validating WA/WF examples..."
	@test -f examples/wa.json && echo "✓ wa.json"
	@test -f examples/wf.json && echo "✓ wf.json"
	@test -f examples/receipt.json && echo "✓ receipt.json"
