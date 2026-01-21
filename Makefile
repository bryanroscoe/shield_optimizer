# Android TV Optimizer - Docker Test Makefile
# For testing the script in a Linux environment

SCRIPT = Shield-Optimizer.ps1
IMAGE = shield-optimizer-test

.PHONY: help build run shell clean

help:
	@echo "Docker Commands for Linux Testing"
	@echo ""
	@echo "  make build   - Build the test image"
	@echo "  make run     - Run the optimizer"
	@echo "  make shell   - Open PowerShell shell"
	@echo "  make clean   - Remove the image"
	@echo ""

build:
	docker build -t $(IMAGE) .

run:
	docker run -it --rm --network host \
		-v "$(PWD)/$(SCRIPT)":/app/$(SCRIPT) \
		-w /app $(IMAGE) pwsh ./$(SCRIPT)

shell:
	docker run -it --rm --network host \
		-v "$(PWD)/$(SCRIPT)":/app/$(SCRIPT) \
		-w /app $(IMAGE) pwsh

clean:
	docker rmi $(IMAGE) 2>/dev/null || true
