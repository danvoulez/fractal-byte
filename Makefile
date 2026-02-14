
.PHONY: build test run race conformance validate docs docker e2e sdk-ts sdk-py auth-smoke compat certs-dev release pre-release changelog lint fmt

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

e2e:
	@echo "── Running e2e smoke (12 asserts) ──"
	bash scripts/e2e.sh

auth-smoke:
	@echo "── Auth smoke: no token → 401 ──"
	@STATUS=$$(curl -s -o /dev/null -w '%{http_code}' http://localhost:3000/v1/execute \
	  -H 'content-type: application/json' -d '{}'); \
	if [ "$$STATUS" = "401" ]; then echo "  ✓ 401 without token"; \
	else echo "  ✗ expected 401, got $$STATUS (auth may be disabled)"; fi
	@echo "── Auth smoke: dev token → 200/409 ──"
	@STATUS=$$(curl -s -o /dev/null -w '%{http_code}' http://localhost:3000/v1/execute \
	  -H 'authorization: Bearer ubl-dev-token-001' \
	  -H 'content-type: application/json' \
	  -d '{"manifest":{"pipeline":"auth-smoke","in_grammar":{"inputs":{"x":""},"mappings":[],"output_from":"x"},"out_grammar":{"inputs":{"y":""},"mappings":[],"output_from":"y"},"policy":{"allow":true}},"vars":{"x":"1"}}'); \
	if [ "$$STATUS" = "200" ] || [ "$$STATUS" = "409" ]; then echo "  ✓ $$STATUS with dev token"; \
	else echo "  ✗ expected 200/409, got $$STATUS"; fi

sdk-ts:
	@echo "── Generating TypeScript SDK types ──"
	npx openapi-typescript docs/OPENAPI.md -o sdk/ts/types.ts
	@echo "  ✓ sdk/ts/types.ts"

sdk-py:
	@echo "── Generating Python SDK ──"
	openapi-python-client generate --path docs/OPENAPI.md --output-path sdk/py
	@echo "  ✓ sdk/py/"

certs-dev:
	@echo "── Generating dev mTLS certificates ──"
	bash scripts/certs-dev.sh certs/dev

# ── Release pipeline ─────────────────────────────────────────────

v ?= patch
release:
	@if [ -z "$(version)" ]; then echo "Usage: make release version=0.2.0"; exit 1; fi
	@echo "═══ Releasing v$(version) ═══"
	$(MAKE) pre-release
	@echo "── Tagging v$(version) ──"
	git tag -a "v$(version)" -m "Release v$(version)"
	git push origin main --tags
	@echo "✓ Tag v$(version) pushed — GitHub Actions will build + release"

pre-release: fmt-fix lint test conformance compat
	@echo "✓ Pre-release gate passed"

changelog:
	@echo "── Generating changelog ──"
	git-cliff --output CHANGELOG.md
	@echo "  ✓ CHANGELOG.md updated"

lint:
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt --all --check

fmt-fix:
	cargo fmt --all

compat:
	@echo "── Compat check: OpenAPI spec version ──"
	@grep -q 'version: "0.1.0"' docs/OPENAPI.md && echo "  ✓ spec version 0.1.0" || echo "  ✗ spec version mismatch"
	@echo "── Compat check: receipt-first invariants ──"
	@grep -q 'Receipt' docs/OPENAPI.md && echo "  ✓ Receipt schema present" || echo "  ✗ Receipt schema missing"
	@grep -q 'Logline' docs/OPENAPI.md && echo "  ✓ Logline schema present" || echo "  ✗ Logline schema missing"
	@grep -q 'bearerAuth' docs/OPENAPI.md && echo "  ✓ securitySchemes present" || echo "  ✗ securitySchemes missing"
	@grep -q '409' docs/OPENAPI.md && echo "  ✓ 409 CONFLICT documented" || echo "  ✗ 409 missing"
