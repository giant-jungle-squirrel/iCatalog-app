param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')]
    [string]$Target
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$version = (Select-String -Path crates/icatalog-cli/Cargo.toml -Pattern '^version = "([^"]+)"$').Matches[0].Groups[1].Value
$uiVersion = (Select-String -Path crates/icatalog-ui/Cargo.toml -Pattern '^version = "([^"]+)"$').Matches[0].Groups[1].Value
if ($version -cne $uiVersion -or $version -cnotmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$') {
    throw 'CLI and UI versions must match (X.Y.Z or X.Y.Z-prerelease, without build metadata).'
}
$binaryDir = if ($env:ICATALOG_RELEASE_BINARY_DIR) { $env:ICATALOG_RELEASE_BINARY_DIR } else { "target/$Target/release" }
$outputDir = if ($env:ICATALOG_RELEASE_OUTPUT_DIR) { $env:ICATALOG_RELEASE_OUTPUT_DIR } else { 'target/release-assets' }
$arch = if ($Target -eq 'x86_64-pc-windows-msvc') { 'x64' } else { 'arm64' }
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
$archive = Join-Path $outputDir "icatalog-$version-windows-$arch.zip"
# Build with the static MSVC runtime (see release.yml), so no redistributable is required.
$contents = @("$binaryDir/icatalog.exe", "$binaryDir/icatalog-ui.exe", 'packaging/windows/associate.ps1', 'packaging/windows/README.txt')
if (Test-Path -LiteralPath 'licenses/craft-libs' -PathType Container) {
    foreach ($required in @('README.md', 'SOURCE.json', 'LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE', 'pdf-backend/hayro-interpret/assets/LICENSE_FOXIT', 'pdf-backend/hayro-cmap/LICENSE.txt')) {
        $notice = "licenses/craft-libs/$required"
        if (!(Test-Path -LiteralPath $notice -PathType Leaf) -or (Get-Item -LiteralPath $notice).Length -eq 0) {
            throw "Missing dependency notice: $notice"
        }
    }
    $contents += 'licenses'
} elseif (Select-String -LiteralPath 'Cargo.lock' -Pattern '^name = "craft-(ui|raw|pdf)"$' -Quiet) {
    throw 'Missing licenses/craft-libs notices required by this application source.'
}
Compress-Archive -LiteralPath $contents -DestinationPath $archive -Force
