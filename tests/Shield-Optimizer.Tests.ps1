# Shield-Optimizer.Tests.ps1 - Pester unit tests for Shield-Optimizer
# Run with: Invoke-Pester -Path ./tests/ -Output Detailed

BeforeAll {
    # Import test helpers
    Import-Module (Join-Path $PSScriptRoot "TestHelpers.psm1") -Force

    # Path to the main script
    $Script:ScriptPath = Join-Path $PSScriptRoot ".." "Shield-Optimizer.ps1"
}

# =============================================================================
# Priority 1: Pure Utility Functions (no dependencies)
# =============================================================================

Describe "Test-ValidIP" -Tag "Unit", "Priority1" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Test-ValidIP"
        . ([ScriptBlock]::Create($funcDef))
    }

    Context "Valid IP addresses" {
        It "Should accept valid IP: <IP>" -ForEach @(
            @{ IP = "192.168.1.1" }
            @{ IP = "10.0.0.1" }
            @{ IP = "172.16.0.1" }
            @{ IP = "255.255.255.255" }
            @{ IP = "0.0.0.0" }
            @{ IP = "1.2.3.4" }
        ) {
            Test-ValidIP -IP $IP | Should -BeTrue
        }

        It "Should accept IP with port: <IP>" -ForEach @(
            @{ IP = "192.168.1.1:5555" }
            @{ IP = "10.0.0.1:5555" }
            @{ IP = "192.168.42.143:5555" }
            @{ IP = "192.168.42.25:5555" }
        ) {
            Test-ValidIP -IP $IP | Should -BeTrue
        }
    }

    Context "Invalid IP addresses" {
        It "Should reject invalid IP: <IP>" -ForEach @(
            @{ IP = "192.168.1.256" }
            @{ IP = "256.1.1.1" }
            @{ IP = "192.168.1" }
            @{ IP = "192.168.1.1.1" }
            @{ IP = "" }
            @{ IP = "abc.def.ghi.jkl" }
            @{ IP = "192.168.1.1:abc" }
            @{ IP = "192.168.1.-1" }
            @{ IP = "not an ip" }
            @{ IP = "192.168.1.1:5555:5555" }
        ) {
            Test-ValidIP -IP $IP | Should -BeFalse
        }

        It "Should reject null IP" {
            Test-ValidIP -IP $null | Should -BeFalse
        }
    }
}

Describe "Format-FileSize" -Tag "Unit", "Priority1" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Format-FileSize"
        . ([ScriptBlock]::Create($funcDef))
    }

    Context "Bytes (under 1KB)" {
        It "Should format <Bytes> bytes as '<Expected>'" -ForEach @(
            @{ Bytes = 0; Expected = "0 bytes" }
            @{ Bytes = 1; Expected = "1 bytes" }
            @{ Bytes = 500; Expected = "500 bytes" }
            @{ Bytes = 1023; Expected = "1023 bytes" }
        ) {
            Format-FileSize -Bytes $Bytes | Should -Be $Expected
        }
    }

    Context "Kilobytes (1KB to under 1MB)" {
        It "Should format <Bytes> bytes as KB" -ForEach @(
            @{ Bytes = 1024; Expected = "1.00 KB" }
            @{ Bytes = 1536; Expected = "1.50 KB" }
            @{ Bytes = 10240; Expected = "10.00 KB" }
            @{ Bytes = 1048575; Expected = "1,024.00 KB" }
        ) {
            Format-FileSize -Bytes $Bytes | Should -Be $Expected
        }
    }

    Context "Megabytes (1MB to under 1GB)" {
        It "Should format <Bytes> bytes as MB" -ForEach @(
            @{ Bytes = 1048576; Expected = "1.00 MB" }
            @{ Bytes = 1572864; Expected = "1.50 MB" }
            @{ Bytes = 10485760; Expected = "10.00 MB" }
            @{ Bytes = 104857600; Expected = "100.00 MB" }
        ) {
            Format-FileSize -Bytes $Bytes | Should -Be $Expected
        }
    }

    Context "Gigabytes (1GB and above)" {
        It "Should format <Bytes> bytes as GB" -ForEach @(
            @{ Bytes = 1073741824; Expected = "1.00 GB" }
            @{ Bytes = 1610612736; Expected = "1.50 GB" }
            @{ Bytes = 10737418240; Expected = "10.00 GB" }
        ) {
            Format-FileSize -Bytes $Bytes | Should -Be $Expected
        }
    }
}

Describe "Get-SubnetFromGateway" -Tag "Unit", "Priority1" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Get-SubnetFromGateway"
        . ([ScriptBlock]::Create($funcDef))
    }

    It "Should extract subnet from <Gateway>" -ForEach @(
        @{ Gateway = "192.168.1.1"; Expected = "192.168.1" }
        @{ Gateway = "10.0.0.1"; Expected = "10.0.0" }
        @{ Gateway = "172.16.0.1"; Expected = "172.16.0" }
        @{ Gateway = "192.168.42.1"; Expected = "192.168.42" }
    ) {
        Get-SubnetFromGateway -Gateway $Gateway | Should -Be $Expected
    }
}

Describe "Test-PackageInList" -Tag "Unit", "Priority1" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Test-PackageInList"
        . ([ScriptBlock]::Create($funcDef))

        # Sample package list (similar to real device output)
        $Script:SamplePackageList = @"
package:com.android.tv.settings
package:com.google.android.tvlauncher
package:com.google.android.youtube.tv
package:com.nvidia.shieldtech.proxy
package:com.example.test.app
"@
    }

    Context "Exact matches" {
        It "Should find existing package: <Package>" -ForEach @(
            @{ Package = "com.android.tv.settings" }
            @{ Package = "com.google.android.tvlauncher" }
            @{ Package = "com.google.android.youtube.tv" }
            @{ Package = "com.nvidia.shieldtech.proxy" }
        ) {
            Test-PackageInList -PackageList $Script:SamplePackageList -Package $Package | Should -BeTrue
        }
    }

    Context "Non-existent packages" {
        It "Should not find missing package: <Package>" -ForEach @(
            @{ Package = "com.nonexistent.app" }
            @{ Package = "com.google.android.tv" }  # Partial match should fail
            @{ Package = "com.google.android.youtube" }  # Partial match should fail
        ) {
            Test-PackageInList -PackageList $Script:SamplePackageList -Package $Package | Should -BeFalse
        }
    }

    Context "Edge cases" {
        It "Should handle empty package list" {
            Test-PackageInList -PackageList "" -Package "com.example.app" | Should -BeFalse
        }

        It "Should handle package with special regex characters" {
            $listWithSpecial = "package:com.example.app+test"
            Test-PackageInList -PackageList $listWithSpecial -Package "com.example.app+test" | Should -BeTrue
        }
    }
}

Describe "Get-UninstallErrorReason" -Tag "Unit", "Priority1" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Get-UninstallErrorReason"
        . ([ScriptBlock]::Create($funcDef))
    }

    It "Should identify 'Broken pipe' as protected system app" {
        Get-UninstallErrorReason "error: Broken pipe" | Should -Be "Protected system app - cannot be removed"
    }

    It "Should identify 'not installed for' as not installed for user" {
        Get-UninstallErrorReason "Failure [not installed for 0]" | Should -Be "App not installed for this user"
    }

    It "Should identify DELETE_FAILED_INTERNAL_ERROR" {
        Get-UninstallErrorReason "Failure [DELETE_FAILED_INTERNAL_ERROR]" | Should -Be "Internal error - app may be in use"
    }

    It "Should return 'Unknown error' for unrecognized patterns" {
        Get-UninstallErrorReason "Some random error" | Should -Be "Unknown error"
    }

    It "Should return 'Unknown error' for empty output" {
        Get-UninstallErrorReason "" | Should -Be "Unknown error"
    }
}

# =============================================================================
# Priority 2: Parsing Functions
# =============================================================================

Describe "Get-ParsedTemperature" -Tag "Unit", "Priority2" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Get-ParsedTemperature"
        . ([ScriptBlock]::Create($funcDef))
    }

    Context "HAL temperature format (priority)" {
        It "Should parse HAL temperature from Shield-style output" {
            $thermalOutput = @"
Current temperatures from HAL:
  Temperature{mValue=45.5, mType=0, mName=CPU, mStatus=0}
  Temperature{mValue=42.0, mType=1, mName=GPU, mStatus=0}
"@
            Get-ParsedTemperature -ThermalOutput $thermalOutput | Should -Be 45.5
        }
    }

    Context "Fallback patterns" {
        It "Should parse mValue with CPU name" {
            $thermalOutput = "Temperature{mValue=52.3, mType=0, mName=CPU0, mStatus=0}"
            Get-ParsedTemperature -ThermalOutput $thermalOutput | Should -Be 52.3
        }

        It "Should parse soc_thermal fallback" {
            $thermalOutput = "Temperature{mValue=48.7, mType=0, mName=soc_thermal, mStatus=0}"
            Get-ParsedTemperature -ThermalOutput $thermalOutput | Should -Be 48.7
        }

        It "Should parse mType=0 fallback" {
            $thermalOutput = "Temperature{mValue=55.2, mType=0, mName=unknown, mStatus=0}"
            Get-ParsedTemperature -ThermalOutput $thermalOutput | Should -Be 55.2
        }
    }

    Context "N/A cases" {
        It "Should return N/A for empty output" {
            Get-ParsedTemperature -ThermalOutput "" | Should -Be "N/A"
        }

        It "Should return N/A for null output" {
            Get-ParsedTemperature -ThermalOutput $null | Should -Be "N/A"
        }

        It "Should return N/A for no matching pattern" {
            Get-ParsedTemperature -ThermalOutput "No temperature data available" | Should -Be "N/A"
        }
    }

    Context "Real fixture data" {
        It "Should parse Shield thermal fixture if available" -Skip:(-not (Test-Path (Join-Path $PSScriptRoot "fixtures/shield-thermal.txt"))) {
            $thermal = Get-Content (Join-Path $PSScriptRoot "fixtures/shield-thermal.txt") -Raw
            $result = Get-ParsedTemperature -ThermalOutput $thermal
            $result | Should -Not -Be "N/A"
            $result | Should -BeOfType [double]
            $result | Should -BeGreaterThan 0
            $result | Should -BeLessThan 100
        }

        It "Should parse Onn thermal fixture if available" -Skip:(-not (Test-Path (Join-Path $PSScriptRoot "fixtures/onn-thermal.txt"))) {
            $thermal = Get-Content (Join-Path $PSScriptRoot "fixtures/onn-thermal.txt") -Raw
            $result = Get-ParsedTemperature -ThermalOutput $thermal
            $result | Should -Not -Be "N/A"
            $result | Should -BeOfType [double]
            $result | Should -BeGreaterThan 0
            $result | Should -BeLessThan 100
        }
    }
}

Describe "Get-ParsedRamInfo" -Tag "Unit", "Priority2" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Get-ParsedRamInfo"
        . ([ScriptBlock]::Create($funcDef))
    }

    Context "Complete RAM info parsing" {
        It "Should parse all RAM fields correctly" {
            $memInfo = @"
Applications Memory Usage (in Kilobytes):
    Uptime: 123456 Realtime: 123456

Total RAM: 3,145,728K
Free RAM: 1,048,576K (  512,000K cached pss +   536,576K free)
Used RAM: 2,097,152K (1,048,576K used pss +   1,048,576K kernel)
ZRAM: 262,144K physical used for 524,288K in swap (1,048,576K total swap)
"@
            $result = Get-ParsedRamInfo -MemInfoOutput $memInfo

            $result.Total | Should -Be 3072   # 3,145,728K / 1024 = 3072MB
            $result.Free | Should -Be 1024    # 1,048,576K / 1024 = 1024MB
            $result.Used | Should -Be 2048    # Total - Free
            $result.Swap | Should -Be 512     # 524,288K / 1024 = 512MB
            $result.Percent | Should -Be 67   # (2048/3072) * 100 = ~67%
        }
    }

    Context "Edge cases" {
        It "Should default to 2048MB total when not found" {
            $result = Get-ParsedRamInfo -MemInfoOutput "Invalid data"
            $result.Total | Should -Be 2048
        }

        It "Should handle empty input" {
            $result = Get-ParsedRamInfo -MemInfoOutput ""
            $result.Total | Should -Be 2048
            $result.Free | Should -Be 0
            $result.Percent | Should -Be 100
        }

        It "Should handle null input" {
            $result = Get-ParsedRamInfo -MemInfoOutput $null
            $result.Total | Should -Be 2048
        }

        It "Should calculate correct percentage" {
            $memInfo = @"
Total RAM: 2,097,152K
Free RAM: 419,430K
"@
            $result = Get-ParsedRamInfo -MemInfoOutput $memInfo
            $result.Percent | Should -Be 80  # 80% used
        }
    }

    Context "Real fixture data" {
        It "Should parse Shield meminfo fixture if available" -Skip:(-not (Test-Path (Join-Path $PSScriptRoot "fixtures/shield-meminfo.txt"))) {
            $memInfo = Get-Content (Join-Path $PSScriptRoot "fixtures/shield-meminfo.txt") -Raw
            $result = Get-ParsedRamInfo -MemInfoOutput $memInfo

            $result.Total | Should -BeGreaterThan 0
            $result.Percent | Should -BeGreaterOrEqual 0
            $result.Percent | Should -BeLessOrEqual 100
        }

        It "Should parse Onn meminfo fixture if available" -Skip:(-not (Test-Path (Join-Path $PSScriptRoot "fixtures/onn-meminfo.txt"))) {
            $memInfo = Get-Content (Join-Path $PSScriptRoot "fixtures/onn-meminfo.txt") -Raw
            $result = Get-ParsedRamInfo -MemInfoOutput $memInfo

            $result.Total | Should -BeGreaterThan 0
            $result.Percent | Should -BeGreaterOrEqual 0
            $result.Percent | Should -BeLessOrEqual 100
        }
    }
}

Describe "Get-VitalColor" -Tag "Unit", "Priority2" {
    BeforeAll {
        # Need to set up VitalThresholds for this function
        $Script:VitalThresholds = @{
            Temperature = @{ Warning = 50; Critical = 70; Normal = "Green"; NormalAnsi = "92" }
            RAM         = @{ Warning = 70; Critical = 85; Normal = "Green"; NormalAnsi = "92" }
            Storage     = @{ Warning = 75; Critical = 90; Normal = "Green"; NormalAnsi = "92" }
            AppMemory   = @{ Warning = 100; Critical = 200; Normal = "White"; NormalAnsi = "97" }
        }

        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Get-VitalColor"
        . ([ScriptBlock]::Create($funcDef))
    }

    Context "Temperature thresholds" {
        It "Should return Green for normal temperature (<= 50)" -ForEach @(
            @{ Value = 30 }
            @{ Value = 45 }
            @{ Value = 50 }
        ) {
            Get-VitalColor -Type "Temperature" -Value $Value | Should -Be "Green"
        }

        It "Should return Yellow for warning temperature (51-70)" -ForEach @(
            @{ Value = 51 }
            @{ Value = 60 }
            @{ Value = 70 }
        ) {
            Get-VitalColor -Type "Temperature" -Value $Value | Should -Be "Yellow"
        }

        It "Should return Red for critical temperature (> 70)" -ForEach @(
            @{ Value = 71 }
            @{ Value = 80 }
            @{ Value = 100 }
        ) {
            Get-VitalColor -Type "Temperature" -Value $Value | Should -Be "Red"
        }
    }

    Context "RAM thresholds" {
        It "Should return Green for normal RAM usage (<= 70%)" {
            Get-VitalColor -Type "RAM" -Value 50 | Should -Be "Green"
        }

        It "Should return Yellow for warning RAM usage (71-85%)" {
            Get-VitalColor -Type "RAM" -Value 75 | Should -Be "Yellow"
        }

        It "Should return Red for critical RAM usage (> 85%)" {
            Get-VitalColor -Type "RAM" -Value 90 | Should -Be "Red"
        }
    }

    Context "Unknown type" {
        It "Should return White for unknown type" {
            Get-VitalColor -Type "UnknownType" -Value 50 | Should -Be "White"
        }
    }
}

Describe "Test-AppPackage" -Tag "Unit", "Priority2" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Test-AppPackage"
        . ([ScriptBlock]::Create($funcDef))
    }

    Context "Valid app package patterns" {
        It "Should identify com.* packages as apps" -ForEach @(
            @{ Package = "com.google.android.youtube.tv" }
            @{ Package = "com.nvidia.shieldtech.proxy" }
            @{ Package = "com.example.app" }
        ) {
            Test-AppPackage -Package $Package | Should -BeTrue
        }

        It "Should identify tv.* packages as apps" -ForEach @(
            @{ Package = "tv.twitch.android.app" }
            @{ Package = "tv.plex.client" }
        ) {
            Test-AppPackage -Package $Package | Should -BeTrue
        }

        It "Should identify me.* packages as apps" -ForEach @(
            @{ Package = "me.myapp.example" }
        ) {
            Test-AppPackage -Package $Package | Should -BeTrue
        }
    }

    Context "Non-app packages (system processes)" {
        It "Should not identify system processes as apps" -ForEach @(
            @{ Package = "android" }
            @{ Package = "system" }
            @{ Package = "org.example.app" }
            @{ Package = "net.example.app" }
            @{ Package = "io.example.app" }
        ) {
            Test-AppPackage -Package $Package | Should -BeFalse
        }
    }
}

# =============================================================================
# Priority 3: Config Functions
# =============================================================================

Describe "Get-DeviceTypeName" -Tag "Unit", "Priority3" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Get-DeviceTypeName"
        . ([ScriptBlock]::Create($funcDef))
    }

    It "Should map 'Shield' to 'Nvidia Shield'" {
        Get-DeviceTypeName -Type "Shield" | Should -Be "Nvidia Shield"
    }

    It "Should map 'GoogleTV' to 'Google TV'" {
        Get-DeviceTypeName -Type "GoogleTV" | Should -Be "Google TV"
    }

    It "Should map unknown types to 'Android TV'" {
        Get-DeviceTypeName -Type "Unknown" | Should -Be "Android TV"
        Get-DeviceTypeName -Type "" | Should -Be "Android TV"
        Get-DeviceTypeName -Type $null | Should -Be "Android TV"
    }
}

Describe "Get-AdbConfig" -Tag "Unit", "Priority3" {
    BeforeAll {
        $funcDef = Get-FunctionDefinition -ScriptPath $Script:ScriptPath -FunctionName "Get-AdbConfig"
        . ([ScriptBlock]::Create($funcDef))
    }

    Context "Windows configuration" {
        BeforeAll {
            $Script:Platform = "Windows"
        }

        It "Should return Windows ADB config" {
            $config = Get-AdbConfig
            $config.BinaryName | Should -Be "adb.exe"
            $config.ExtraFiles | Should -Contain "AdbWinApi.dll"
            $config.ExtraFiles | Should -Contain "AdbWinUsbApi.dll"
            $config.DownloadUrl | Should -Match "windows"
        }
    }

    Context "macOS configuration" {
        BeforeAll {
            $Script:Platform = "macOS"
        }

        It "Should return macOS ADB config" {
            $config = Get-AdbConfig
            $config.BinaryName | Should -Be "adb"
            $config.ExtraFiles.Count | Should -Be 0
            $config.DownloadUrl | Should -Match "darwin"
        }
    }

    Context "Linux configuration" {
        BeforeAll {
            $Script:Platform = "Linux"
        }

        It "Should return Linux ADB config" {
            $config = Get-AdbConfig
            $config.BinaryName | Should -Be "adb"
            $config.ExtraFiles.Count | Should -Be 0
            $config.DownloadUrl | Should -Match "linux"
        }
    }

    Context "Default/fallback configuration" {
        BeforeAll {
            $Script:Platform = "Unknown"
        }

        It "Should fallback to Windows config for unknown platform" {
            $config = Get-AdbConfig
            $config.BinaryName | Should -Be "adb.exe"
        }
    }
}
