# Dev-only helpers. The real build lives in Cargo; these are conveniences for
# local testing (see scripts/seed.sh).

SEED_DATA_DIR ?= ./.seed

.PHONY: seed seed-tui seed-clean

# Build cdx and populate a throwaway catalog with fake books for TUI testing.
seed:
	cargo build
	SEED_DATA_DIR=$(SEED_DATA_DIR) ./scripts/seed.sh

# Launch the TUI against the seeded catalog.
seed-tui:
	target/debug/cdx --data-dir $(SEED_DATA_DIR) tui

# Remove the seeded catalog and its registry.
seed-clean:
	rm -rf $(SEED_DATA_DIR)
