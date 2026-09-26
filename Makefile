.PHONY: build test test-vault test-registry check-interface-docs check-event-field-types test-gas-budget-check install-hooks deploy-testnet

build:
	stellar contract build

# Wire scripts/hooks/pre-commit into git so `cargo fmt --check` runs before every commit (#251).
install-hooks:
	git config core.hooksPath scripts/hooks
	@echo "Pre-commit hook installed: cargo fmt --check now runs before every commit."

test:
	cargo test

# Scope to a single crate for faster local iteration (#254).
test-vault:
	cargo test -p investment-vault

test-registry:
	cargo test -p project-registry

check-interface-docs:
	python3 scripts/check_interface_docs.py

check-event-field-types:
	python3 scripts/check_event_field_types.py

test-gas-budget-check:
	python3 -m unittest discover -s scripts/tests

deploy-testnet: test build
	@echo "Checking WASM sizes (budget: 131072 bytes)..."
	@FAILED=0; \
	for f in target/wasm32v1-none/release/*.wasm; do \
	  name=$$(basename "$$f" .wasm); \
	  size=$$(wc -c < "$$f"); \
	  printf "  %-40s %7d bytes\n" "$$name" "$$size"; \
	  if [ "$$size" -gt 131072 ]; then \
	    echo "FAILED: $$name exceeds budget"; \
	    FAILED=1; \
	  fi; \
	done; \
	[ "$$FAILED" -eq 0 ] || exit 1
	@mkdir -p deploy
	@REGISTRY_ID=$$(stellar contract deploy \
	  --wasm target/wasm32v1-none/release/project_registry.wasm \
	  --source $(STELLAR_SECRET_KEY) \
	  --network testnet \
	  -- \
	  --admin $(ADMIN_ADDRESS) \
	  --whitelister $(WHITELISTER_ADDRESS)) && \
	echo "ProjectRegistry: $$REGISTRY_ID" && \
	VAULT_ID=$$(stellar contract deploy \
	  --wasm target/wasm32v1-none/release/investment_vault.wasm \
	  --source $(STELLAR_SECRET_KEY) \
	  --network testnet \
	  -- \
	  --admin $(ADMIN_ADDRESS) \
	  --usdc_sac $(USDC_SAC_ADDRESS) \
	  --registry $$REGISTRY_ID) && \
	echo "InvestmentVault: $$VAULT_ID" && \
	printf '{"network":"testnet","project_registry":"%s","investment_vault":"%s"}\n' \
	  "$$REGISTRY_ID" "$$VAULT_ID" > deploy/testnet.json && \
	python3 scripts/check_deploy_wasm_hash.py update deploy/testnet.json \
	  project_registry=target/wasm32v1-none/release/project_registry.wasm \
	  investment_vault=target/wasm32v1-none/release/investment_vault.wasm && \
	echo "Saved to deploy/testnet.json (contract IDs + WASM hashes)"
