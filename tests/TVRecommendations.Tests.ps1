BeforeAll {
    Import-Module (Join-Path $PSScriptRoot "TestHelpers.psm1") -Force
    $script:SourcePath = Join-Path $PSScriptRoot ".." "Shield-Optimizer.ps1"
    $tokens = $null
    $parseErrors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($script:SourcePath, [ref]$tokens, [ref]$parseErrors)
    if ($parseErrors.Count -gt 0) { throw "Production script must parse" }
    $assignment = $ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.AssignmentStatementAst] -and
        $node.Left -is [System.Management.Automation.Language.VariableExpressionAst] -and
        $node.Left.VariablePath.UserPath -eq "Script:ShieldAppList"
    }, $true) | Select-Object -First 1
    if (-not $assignment) { throw "SHIELD catalog assignment not found" }
    $commands = $assignment.Right.FindAll({ param($node) $node -is [System.Management.Automation.Language.CommandAst] }, $true)
    if ($commands.Count -ne 0) { throw "Catalog fixture must contain only data" }
    $entries = & ([ScriptBlock]::Create($assignment.Right.Extent.Text))
    $script:RecommendationsEntry = $entries | Where-Object Package -EQ "com.google.android.tvrecommendations"
    foreach ($name in @("Run-Task", "Get-AppListForDevice", "Get-DeviceTypeName", "Test-PackageInList", "Write-Header", "Write-Info", "Write-Dim", "Read-Toggle", "Get-AppMemoryMap", "Invoke-AdbCommand")) {
        . ([ScriptBlock]::Create((Get-FunctionDefinition -ScriptPath $script:SourcePath -FunctionName $name)))
    }
    function Invoke-CatalogFixtureAdb {
        if ($args -contains "-d") { return "" }
        return "package:com.google.android.tvrecommendations"
    }
}

Describe "Legacy TV Recommendations conservative default" {
    BeforeEach {
        $script:CommonAppList = @()
        $script:ShieldAppList = @($script:RecommendationsEntry)
        $script:GoogleTVAppList = @()
        $script:Colors = @{ Label = "White" }
        $script:AdbPath = "Invoke-CatalogFixtureAdb"
        Mock Get-AppMemoryMap { @{} }
        Mock Write-Host {}
        Mock Write-Info {}
        Mock Write-Dim {}
        Mock Write-Header {
            param($Text)
            if ($Text -like "Performance Settings*") { throw "Package fixture complete" }
        }
        Mock Invoke-AdbCommand { throw "Unexpected mutation command" }
    }

    It "keeps the shipped entry opt-in and retains restoration" {
        $script:RecommendationsEntry.DefaultOptimize | Should -Be "N"
        $script:RecommendationsEntry.DefaultRestore | Should -Be "Y"
        $script:RecommendationsEntry.Method | Should -Be "DISABLE"
        $script:RecommendationsEntry.Risk | Should -Be "High Risk"
        foreach ($phrase in @("phone app", "reboot", "custom launcher", "darcy", "foster_e", "Android 11", "unverified")) {
            $script:RecommendationsEntry.OptimizeDescription | Should -Match ([regex]::Escape($phrase))
        }
        $script:RecommendationsEntry.OptimizeDescription | Should -Not -Match "No effect"
    }

    It "skips an enabled package with ApplyAll=<ApplyAll> and displays its consequence" -ForEach @(
        @{ ApplyAll = 0 }
        @{ ApplyAll = 1 }
    ) {
        $script:ApplyAllFixture = $ApplyAll
        Mock Read-Toggle {
            param($Prompt, $Options, $DefaultIndex)
            if ($Prompt -like "Apply all default actions*") { return $script:ApplyAllFixture }
            return $DefaultIndex
        }
        { Run-Task -Target "simulated-not-a-device" -Mode "Optimize" -DeviceType "Shield" } |
            Should -Throw "*Package fixture complete*"
        $script:Summary.Skipped | Should -Be 1
        $script:Summary.Disabled | Should -Be 0
        $script:Summary.Uninstalled | Should -Be 0
        Should -Invoke Invoke-AdbCommand -Times 0 -Exactly
        Should -Invoke Write-Dim -Times 1 -Exactly -ParameterFilter { $Text -match "phone app" -and $Text -match "reboot" }
    }
}
