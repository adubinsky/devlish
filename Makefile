# Contributor tooling stays here; installed users run only devlish.
CARGO ?= cargo
PREFIX ?= $(HOME)/.local
DESTDIR ?=
MANIFEST := crates/devlish_core/Cargo.toml
BINARY := crates/devlish_core/target/release/devlish-core

.PHONY: all build install test
all: build

build:
	@echo "Building Devlish..."
	@$(CARGO) build --locked --release --manifest-path $(MANIFEST)

install: build
	@install -d "$(DESTDIR)$(PREFIX)/bin"
	@install -m 755 "$(BINARY)" "$(DESTDIR)$(PREFIX)/bin/devlish"
	@echo "Installed $(DESTDIR)$(PREFIX)/bin/devlish"
	@echo "Run devlish, devlish --run FILE, or devlish --server."

test:
	@$(CARGO) test --locked --manifest-path $(MANIFEST)
