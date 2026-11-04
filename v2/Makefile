.PHONY: dev brew setup

dev:
	@command -v node >/dev/null 2>&1 || { echo "Node not found. Run: brew install node"; exit 1; }
	@command -v cargo >/dev/null 2>&1 || { echo "Rust not found. Run: make setup"; exit 1; }
	npm install
	npm run tauri dev

brew:
	brew install --cask bryanroscoe/shield-optimizer/shield-optimizer

setup:
	@command -v brew >/dev/null 2>&1 || { echo "Homebrew not found. Install from https://brew.sh"; exit 1; }
	brew install node
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
	@echo "Run 'source $$HOME/.cargo/env' then 'make dev'"
