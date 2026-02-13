
.PHONY: build test run race conformance validate docs docker

build:
	cargo build --workspace

test:
	cargo test --workspace

race:
	cargo test -p ubl_gate --test race_card -- --nocapture

conformance:
	cargo test -p ubl_runtime -- --nocapture

validate: test
	./scripts/check-cids.sh

docs:
	@[ -f docs/RUNTIME_BLUEPRINT.md ] && echo "✓ runtime blueprint"
	@[ -f docs/runtime-ghost-write-ahead.md ] && echo "✓ ghost/write-ahead docs"
	@[ -f examples/wa.json ] && [ -f examples/wf.json ] && [ -f examples/receipt.json ] && echo "✓ WA/WF/receipt examples"

run:
	REGISTRY_BASE_URL=http://localhost:3000 cargo run -p ubl_gate

docker:
	docker build -t ubl-registry:latest .
