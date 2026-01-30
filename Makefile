# Shield-Optimizer Makefile
# Build/test automation for the project

SHELL := /bin/bash
ADB := ./platform-tools/adb
SHIELD := 192.168.42.143:5555
ONN := 192.168.42.25:5555

.PHONY: test test-verbose test-coverage fixtures lint help connect clean screenshots screenshots-auto

# Default target
help:
	@echo "Shield-Optimizer - Available targets:"
	@echo ""
	@echo "  Testing:"
	@echo "    test          - Run all Pester tests"
	@echo "    test-verbose  - Run tests with detailed output"
	@echo "    test-coverage - Run tests with code coverage"
	@echo "    lint          - Check PowerShell syntax"
	@echo ""
	@echo "  Fixtures:"
	@echo "    fixtures      - Gather fresh test fixtures from all devices"
	@echo "    fixtures-shield - Gather fixtures from Shield TV only"
	@echo "    fixtures-onn    - Gather fixtures from Onn 4K Pro only"
	@echo ""
	@echo "  Device:"
	@echo "    connect       - Connect to both devices via ADB"
	@echo ""
	@echo "  Screenshots:"
	@echo "    screenshots     - Interactive screenshot gallery (N/P/Q to navigate)"
	@echo "    screenshots-auto - Automated PNG capture (requires: brew install homeport/tap/termshot)"
	@echo ""
	@echo "  Cleanup:"
	@echo "    clean         - Remove generated files"

# Run tests
test:
	pwsh -NoProfile -Command "Invoke-Pester -Path ./tests/ -Output Normal"

test-verbose:
	pwsh -NoProfile -Command "Invoke-Pester -Path ./tests/ -Output Detailed"

test-coverage:
	pwsh -NoProfile -Command "Invoke-Pester -Path ./tests/ -Output Detailed -CodeCoverage ./Shield-Optimizer.ps1"

# Gather real device data for test fixtures
fixtures: fixtures-shield fixtures-onn

fixtures-shield:
	@echo "Gathering Shield TV fixtures..."
	@mkdir -p tests/fixtures
	$(ADB) -s $(SHIELD) shell dumpsys thermalservice > tests/fixtures/shield-thermal.txt
	$(ADB) -s $(SHIELD) shell dumpsys meminfo > tests/fixtures/shield-meminfo.txt
	$(ADB) -s $(SHIELD) shell pm list packages > tests/fixtures/shield-packages.txt
	$(ADB) -s $(SHIELD) shell pm list packages -d > tests/fixtures/shield-disabled.txt
	@echo "Shield fixtures saved."

fixtures-onn:
	@echo "Gathering Onn 4K Pro fixtures..."
	@mkdir -p tests/fixtures
	$(ADB) -s $(ONN) shell dumpsys thermalservice > tests/fixtures/onn-thermal.txt
	$(ADB) -s $(ONN) shell dumpsys meminfo > tests/fixtures/onn-meminfo.txt
	$(ADB) -s $(ONN) shell pm list packages > tests/fixtures/onn-packages.txt
	$(ADB) -s $(ONN) shell pm list packages -d > tests/fixtures/onn-disabled.txt
	@echo "Onn fixtures saved."

# Connect to devices
connect:
	$(ADB) connect $(SHIELD)
	$(ADB) connect $(ONN)

# Syntax check
lint:
	@pwsh -NoProfile -Command "\
		\$$errors = \$$null; \
		[System.Management.Automation.Language.Parser]::ParseFile('Shield-Optimizer.ps1', [ref]\$$null, [ref]\$$errors) | Out-Null; \
		if (\$$errors.Count -gt 0) { \$$errors | ForEach-Object { Write-Host \$$_.ToString() -ForegroundColor Red }; exit 1 } \
		else { Write-Host 'Syntax OK' -ForegroundColor Green }"

# Clean generated files
clean:
	rm -f test-results.xml coverage.xml
	rm -rf TestResults/

# Screenshots / Demo mode
screenshots:
	pwsh -NoProfile -File ./demos/ScreenshotGallery.ps1

# Automated PNG capture (requires: brew install homeport/tap/termshot)
SCREENSHOT_NAMES := main-menu action-menu device-profile health-vitals top-memory bloat-check launcher-wizard help-screen optimize-prompt task-summary

screenshots-auto:
	@mkdir -p screenshots
	@i=1; for name in $(SCREENSHOT_NAMES); do \
		echo "Capturing $$name..."; \
		termshot -f screenshots/$$name.png -- pwsh -NoProfile -File ./demos/ScreenshotGallery.ps1 -Screen $$i; \
		i=$$((i + 1)); \
	done
	@echo "Done! Screenshots saved to ./screenshots/"
