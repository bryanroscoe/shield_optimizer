# TestHelpers.psm1 - Helper module for extracting functions from Shield-Optimizer.ps1
# Allows testing individual functions without executing script side effects

function Get-FunctionDefinition {
    <#
    .SYNOPSIS
        Extracts a function definition from a PowerShell script file.
    .DESCRIPTION
        Uses PowerShell's AST parser to extract a complete function definition,
        allowing the function to be loaded in isolation for testing.
    .PARAMETER ScriptPath
        Path to the PowerShell script file.
    .PARAMETER FunctionName
        Name of the function to extract.
    .OUTPUTS
        String containing the complete function definition.
    #>
    param(
        [Parameter(Mandatory)]
        [string]$ScriptPath,

        [Parameter(Mandatory)]
        [string]$FunctionName
    )

    # Resolve relative paths
    if (-not [System.IO.Path]::IsPathRooted($ScriptPath)) {
        $ScriptPath = Join-Path $PSScriptRoot $ScriptPath
    }

    if (-not (Test-Path $ScriptPath)) {
        throw "Script file not found: $ScriptPath"
    }

    # Use PowerShell AST parser for reliable function extraction
    $tokens = $null
    $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($ScriptPath, [ref]$tokens, [ref]$errors)

    # Find the function definition by name
    $funcAst = $ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -eq $FunctionName
    }, $true) | Select-Object -First 1

    if (-not $funcAst) {
        throw "Function '$FunctionName' not found in $ScriptPath"
    }

    # Return the complete function text
    return $funcAst.Extent.Text
}

function Import-FunctionFromScript {
    <#
    .SYNOPSIS
        Extracts and imports a function from a script into the current session.
    .DESCRIPTION
        Combines Get-FunctionDefinition with ScriptBlock creation to load
        a function in isolation for testing.
    .PARAMETER ScriptPath
        Path to the PowerShell script file.
    .PARAMETER FunctionName
        Name of the function to import.
    #>
    param(
        [Parameter(Mandatory)]
        [string]$ScriptPath,

        [Parameter(Mandatory)]
        [string]$FunctionName
    )

    $funcDef = Get-FunctionDefinition -ScriptPath $ScriptPath -FunctionName $FunctionName
    . ([ScriptBlock]::Create($funcDef))
}

function Get-FixturePath {
    <#
    .SYNOPSIS
        Returns the full path to a test fixture file.
    .PARAMETER FixtureName
        Name of the fixture file (e.g., "shield-thermal.txt").
    #>
    param(
        [Parameter(Mandatory)]
        [string]$FixtureName
    )

    return Join-Path $PSScriptRoot "fixtures" $FixtureName
}

function Get-FixtureContent {
    <#
    .SYNOPSIS
        Reads and returns the content of a test fixture file.
    .PARAMETER FixtureName
        Name of the fixture file.
    #>
    param(
        [Parameter(Mandatory)]
        [string]$FixtureName
    )

    $path = Get-FixturePath -FixtureName $FixtureName
    if (-not (Test-Path $path)) {
        throw "Fixture file not found: $path"
    }
    return Get-Content -Path $path -Raw
}

Export-ModuleMember -Function Get-FunctionDefinition, Import-FunctionFromScript, Get-FixturePath, Get-FixtureContent
